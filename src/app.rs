//! UI: a status-aware sidebar (EuroScope, VC++, pack, configuration, backups) plus a content panel.

mod backups_ui;
mod config_ui;
mod euroscope_ui;
mod recheck;
mod style;
mod update_ui;
mod vcredist_ui;

use crate::{
    backup,
    config::{self, Config},
    download, euroscope,
    manifest::Manifest,
    pack, selfupdate,
    state::State,
    vcredist,
};

use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    EuroScope,
    VcRedist,
    ControllerPack,
    Configure,
    Backups,
}

const STEPS: &[(Step, &str)] = &[
    (Step::EuroScope, "EuroScope"),
    (Step::VcRedist, "VC++ Runtime"),
    (Step::ControllerPack, "Controller Pack"),
    (Step::Configure, "Configuration"),
    (Step::Backups, "Backups"),
];

/// Install steps that must be satisfied, in the order they are walked through.
const INSTALL_STEPS: [Step; 3] = [Step::EuroScope, Step::VcRedist, Step::ControllerPack];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Health {
    Ok,
    Outdated,
    Missing,
}

#[derive(Clone, Copy, Default)]
struct UpdateOptions {
    backup: bool,
    save_layout: bool,
    apply_config: bool,
}

pub struct App {
    confirm_update: bool,
    update_opts: UpdateOptions,
    backups: Vec<backup::Backup>,
    backups_dirty: bool,
    selected_backup: Option<(std::path::PathBuf, Result<Vec<String>, String>)>,
    step: Step,
    state: State,
    applied_theme: Option<egui::Theme>,
    manifest: Option<Manifest>,
    manifest_error: Option<String>,
    self_update_check: Option<std::thread::JoinHandle<anyhow::Result<Option<selfupdate::Release>>>>,
    self_update: Option<selfupdate::Release>,
    euroscope: Option<euroscope::Detected>,
    vcredist: bool,
    rechecking: Option<(&'static str, std::time::Instant)>,
    checked: Option<(&'static str, std::time::Instant)>,
    status: String,
    job: Option<Job>,
    config: Config,
    capturing_asel: bool,
    config_tab: ConfigTab,
    textures: std::collections::HashMap<String, egui::TextureHandle>,
}

#[derive(Clone, Copy, PartialEq)]
enum ConfigTab {
    Details,
    Appearance,
    Controlling,
    Layout,
}

impl ConfigTab {
    const ALL: [(ConfigTab, &'static str); 4] = [
        (ConfigTab::Details, "Your details"),
        (ConfigTab::Appearance, "Appearance"),
        (ConfigTab::Controlling, "Controlling"),
        (ConfigTab::Layout, "Screen layout"),
    ];
}

struct Job {
    shared: download::Shared,
    handle: std::thread::JoinHandle<anyhow::Result<()>>,
}

impl App {
    fn step_description(&self) -> &'static str {
        match self.step {
            Step::EuroScope => "The UK pack requires EuroScope version 3.2.3.2 exactly",
            Step::VcRedist => "The UK Controller Plugin needs this runtime",
            Step::ControllerPack => "Install or update the UK controller pack",
            Step::Configure => "Your settings are saved and re-applied after every pack update",
            Step::Backups => "Restore a saved copy of your controller pack",
        }
    }

    fn health(&self, step: Step) -> Health {
        match step {
            Step::EuroScope => match (&self.euroscope, self.euroscope_ok()) {
                (None, _) => Health::Missing,
                (Some(_), true) => Health::Ok,
                (Some(_), false) => Health::Outdated,
            },
            Step::VcRedist => {
                if self.vcredist {
                    Health::Ok
                } else {
                    Health::Missing
                }
            }
            Step::ControllerPack => match (
                self.state.pack_version,
                self.manifest.as_ref().and_then(Manifest::latest),
            ) {
                (None, _) => Health::Missing,
                (Some(installed), Some(latest)) if installed != latest.version => Health::Outdated,
                (Some(_), _) => Health::Ok,
            },
            Step::Configure | Step::Backups => Health::Ok,
        }
    }

    /// First install step that is missing or outdated, if any.
    fn first_pending(&self) -> Option<Step> {
        INSTALL_STEPS
            .into_iter()
            .find(|s| self.health(*s) != Health::Ok)
    }

    /// Land on the first problem page, or on Configuration when everything is current.
    fn goto_pending_or_configure(&mut self) {
        self.step = self.first_pending().unwrap_or(Step::Configure);
    }

    pub fn new(ctx: egui::Context) -> Self {
        let mut app = App {
            confirm_update: false,
            update_opts: UpdateOptions {
                backup: true,
                save_layout: false,
                apply_config: true,
            },
            backups: Vec::new(),
            backups_dirty: true,
            selected_backup: None,
            step: Step::EuroScope,
            state: State::load(),
            applied_theme: None,
            manifest: None,
            manifest_error: None,
            self_update_check: Some(std::thread::spawn(selfupdate::check)),
            self_update: None,
            euroscope: None,
            vcredist: false,
            rechecking: None,
            checked: None,
            status: String::new(),
            job: None,
            config: Config::default(),
            capturing_asel: false,
            config_tab: ConfigTab::Details,
            textures: Default::default(),
        };
        app.refresh();
        app.reload_state();
        app.config = config::load(app.pack_dir().as_deref());
        app.goto_pending_or_configure();
        ctx.set_theme(app.state.theme.egui());
        app.apply_style(&ctx);
        app
    }

    /// Re-run all detection checks.
    fn refresh(&mut self) {
        self.euroscope = euroscope::detect();
        self.vcredist = vcredist::is_installed();
        match Manifest::fetch() {
            Ok(m) => {
                self.manifest = Some(m);
                self.manifest_error = None;
            }
            Err(e) => self.manifest_error = Some(e.to_string()),
        }
    }

    /// Move on to the next install step needing attention, else Configuration.
    fn next(&mut self) {
        self.goto_pending_or_configure();
    }

    /// Reload persisted paths and derive the installed version from disk.
    fn reload_state(&mut self) {
        self.state = State::load();
        self.refresh_pack_version();
    }

    /// The pack's on-disk version marker is authoritative; saved state alone does not mean it exists.
    fn refresh_pack_version(&mut self) {
        self.state.pack_version = self.pack_dir().and_then(|d| pack::installed_version(&d));
    }

    fn pack_dir(&self) -> Option<std::path::PathBuf> {
        self.state.pack_dir.clone().or_else(pack::default_dir)
    }

    fn euroscope_ok(&self) -> bool {
        let Some(d) = &self.euroscope else {
            return false;
        };
        match &self.manifest {
            Some(m) => d
                .version
                .as_deref()
                .is_some_and(|v| euroscope::is_required_version(v, &m.euroscope.required_version)),
            None => true,
        }
    }

    /// Show progress for a running job and collect its result once finished.
    fn poll_job(&mut self, ui: &mut egui::Ui) {
        let Some(job) = &self.job else { return };
        if job.handle.is_finished() {
            let job = self.job.take().unwrap();
            let result = job.handle.join();
            let succeeded = matches!(result, Ok(Ok(())));
            self.status = match result {
                Ok(Ok(())) => String::new(),
                Ok(Err(e)) => format!("Failed: {e}"),
                Err(_) => "Worker thread panicked".into(),
            };
            self.reload_state();
            self.backups_dirty = true;
            self.euroscope = euroscope::detect();
            self.vcredist = vcredist::is_installed();
            if succeeded && INSTALL_STEPS.contains(&self.step) {
                self.goto_pending_or_configure();
            }
        } else {
            ui.ctx().request_repaint();
        }
    }

    fn progress_ui(&self, ui: &mut egui::Ui) {
        if let Some(job) = &self.job {
            let r = job.shared.lock().unwrap().clone();
            ui.label(&r.message);
            match r.total {
                Some(t) if t > 0 => {
                    ui.add(egui::ProgressBar::new(r.done as f32 / t as f32).show_percentage());
                }
                _ => {
                    ui.add(egui::Spinner::new());
                }
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.applied_theme != Some(ui.ctx().theme()) {
            self.apply_style(ui.ctx());
        }
        self.poll_job(ui);
        if self
            .self_update_check
            .as_ref()
            .is_some_and(|h| h.is_finished())
        {
            if let Ok(Ok(release)) = self.self_update_check.take().unwrap().join() {
                self.self_update = release;
            }
        } else if self.self_update_check.is_some() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
        let busy = self.job.is_some();
        if busy && ui.ctx().input(|i| i.viewport().close_requested()) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.status = "Please wait for the current task to finish before closing.".into();
        }
        egui::Panel::left("steps")
            .resizable(false)
            .default_size(232.0)
            .size_range(232.0..=232.0)
            .frame(
                egui::Frame::new()
                    .fill(if ui.visuals().dark_mode {
                        egui::Color32::from_rgb(30, 34, 40)
                    } else {
                        egui::Color32::from_rgb(239, 242, 246)
                    })
                    .inner_margin(egui::Margin::same(20)),
            )
            .show(ui, |ui| {
                ui.heading(egui::RichText::new("VATSIM UK").strong().size(20.0));
                ui.label(
                    egui::RichText::new("CONTROLLER PACK")
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
                ui.add_space(18.0);
                ui.label(
                    egui::RichText::new("YOUR INSTALLATION")
                        .small()
                        .strong()
                        .color(ui.visuals().weak_text_color()),
                );
                ui.add_space(8.0);
                let dark = ui.visuals().dark_mode;
                for (s, name) in STEPS {
                    let active = self.step == *s;
                    let health = self.health(*s);
                    let (marker, color) = match (*s, health) {
                        (Step::Configure | Step::Backups, _) => ("", ui.visuals().text_color()),
                        (_, Health::Ok) => ("✔  ", Self::status_color(health, dark)),
                        (_, Health::Outdated) => ("!  ", Self::status_color(health, dark)),
                        (_, Health::Missing) => ("•  ", Self::status_color(health, dark)),
                    };
                    let mut text = egui::RichText::new(format!("{marker}{name}")).color(color);
                    if active || health == Health::Outdated {
                        text = text.strong();
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::selectable(active, text))
                        .clicked()
                    {
                        self.step = *s;
                        self.backups_dirty = true;
                    }
                }
                ui.add_space(8.0);
                if ui
                    .add_enabled(!busy, egui::Button::new("Check again"))
                    .clicked()
                {
                    self.refresh();
                    self.reload_state();
                    self.goto_pending_or_configure();
                }
                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.weak("EuroScope 3.2.3.2");
                    ui.horizontal(|ui| {
                        ui.label("Theme");
                        self.theme_ui(ui);
                    });
                });
            });

        egui::Panel::bottom("footer")
            .frame(
                egui::Frame::new()
                    .fill(if ui.visuals().dark_mode {
                        egui::Color32::from_rgb(30, 34, 40)
                    } else {
                        egui::Color32::from_rgb(239, 242, 246)
                    })
                    .inner_margin(egui::Margin::symmetric(22, 12)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if !self.status.is_empty() {
                        let is_error = self.status.starts_with("Failed:")
                            || self.status.starts_with("Could not")
                            || self.status.starts_with("Cannot")
                            || self.status.starts_with("No ");
                        ui.label(egui::RichText::new(&self.status).color(if is_error {
                            ui.visuals().error_fg_color
                        } else {
                            ui.visuals().text_color()
                        }));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if INSTALL_STEPS.contains(&self.step)
                            && self.health(self.step) == Health::Ok
                            && ui
                                .add_enabled(!busy, egui::Button::new("Continue"))
                                .clicked()
                        {
                            self.next();
                        }
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(egui::Margin::same(24)),
            )
            .show(ui, |ui| {
                ui.heading(match self.step {
                    Step::EuroScope => "EuroScope",
                    Step::VcRedist => "Microsoft Visual C++",
                    Step::ControllerPack => "UK Controller Pack",
                    Step::Configure => "Configuration",
                    Step::Backups => "Backups",
                });
                ui.label(
                    egui::RichText::new(self.step_description())
                        .color(ui.visuals().weak_text_color()),
                );
                if let Some(release) = self.self_update.clone() {
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!(
                                "Installer update available: {} (you have {})",
                                release.version,
                                env!("CARGO_PKG_VERSION")
                            ),
                        );
                        if ui
                            .add_enabled(!busy, egui::Button::new("Update and restart"))
                            .clicked()
                        {
                            let shared = download::Shared::default();
                            let worker = shared.clone();
                            let handle =
                                std::thread::spawn(move || selfupdate::apply(&release, &worker));
                            self.job = Some(Job { shared, handle });
                            self.status.clear();
                        }
                    });
                    self.progress_ui(ui);
                }
                if let Some(e) = &self.manifest_error {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!("Could not check for updates: {e}"),
                    );
                }
                if self.step == Step::Configure && self.first_pending().is_none() {
                    ui.colored_label(
                        Self::status_color(Health::Ok, ui.visuals().dark_mode),
                        "✔ EuroScope, VC++ and the controller pack are all up to date",
                    );
                }
                ui.add_space(16.0);
                egui::Frame::new().show(ui, |ui| match self.step {
                    Step::EuroScope => {
                        self.euroscope_ui(ui);
                    }
                    Step::VcRedist => {
                        self.vcredist_ui(ui);
                    }
                    Step::ControllerPack => {
                        self.pack_ui(ui);
                    }
                    Step::Configure => {
                        config_ui::show(self, ui);
                    }
                    Step::Backups => {
                        self.backups_ui(ui);
                    }
                });
            });
    }
}
