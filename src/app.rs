//! UI: a status-aware sidebar (EuroScope, VC++, pack, configuration, backups) plus a content panel.

use crate::{
    backup,
    config::{self, Config},
    download, euroscope, layout,
    manifest::Manifest,
    pack, selfupdate,
    state::{State, ThemePreference},
    updater, vcredist,
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

    fn status_color(health: Health, dark_mode: bool) -> egui::Color32 {
        match (health, dark_mode) {
            (Health::Ok, true) => egui::Color32::from_rgb(110, 190, 145),
            (Health::Ok, false) => egui::Color32::from_rgb(32, 126, 78),
            (Health::Outdated, true) => egui::Color32::from_rgb(240, 200, 70),
            (Health::Outdated, false) => egui::Color32::from_rgb(176, 124, 0),
            (Health::Missing, true) => egui::Color32::from_rgb(158, 165, 174),
            (Health::Missing, false) => egui::Color32::from_rgb(99, 108, 119),
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

    fn apply_style(&mut self, ctx: &egui::Context) {
        let theme = ctx.theme();
        ctx.all_styles_mut(|style| {
            use egui::{Color32, FontId, Stroke, TextStyle};
            for (text_style, size) in [
                (TextStyle::Body, 16.0),
                (TextStyle::Button, 16.0),
                (TextStyle::Small, 13.0),
                (TextStyle::Monospace, 15.0),
                (TextStyle::Heading, 26.0),
            ] {
                style
                    .text_styles
                    .insert(text_style, FontId::proportional(size));
            }
            style.spacing.item_spacing = [12.0, 10.0].into();
            style.spacing.button_padding = [14.0, 8.0].into();
            style.spacing.interact_size.y = 34.0;
            let mut visuals = theme.default_visuals();
            if theme == egui::Theme::Dark {
                visuals.panel_fill = Color32::from_rgb(25, 28, 33);
                visuals.window_fill = Color32::from_rgb(32, 36, 42);
                visuals.extreme_bg_color = Color32::from_rgb(19, 22, 26);
                visuals.faint_bg_color = Color32::from_rgb(39, 44, 51);
                visuals.selection.bg_fill = Color32::from_rgb(47, 71, 96);
                visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(117, 169, 222));
                visuals.widgets.inactive.bg_fill = Color32::from_rgb(39, 44, 51);
                visuals.widgets.inactive.bg_stroke =
                    Stroke::new(1.0, Color32::from_rgb(57, 64, 73));
                visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 56, 66);
                visuals.widgets.hovered.bg_stroke =
                    Stroke::new(1.0, Color32::from_rgb(91, 117, 145));
                visuals.widgets.active.bg_fill = Color32::from_rgb(47, 71, 96);
                visuals.widgets.active.bg_stroke =
                    Stroke::new(1.0, Color32::from_rgb(117, 169, 222));
            } else {
                visuals.panel_fill = Color32::from_rgb(250, 251, 253);
                visuals.window_fill = Color32::WHITE;
                visuals.extreme_bg_color = Color32::WHITE;
                visuals.faint_bg_color = Color32::from_rgb(239, 242, 246);
                visuals.selection.bg_fill = Color32::from_rgb(218, 232, 248);
                visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(54, 105, 160));
                visuals.widgets.inactive.bg_fill = Color32::from_rgb(244, 246, 249);
                visuals.widgets.inactive.bg_stroke =
                    Stroke::new(1.0, Color32::from_rgb(210, 216, 224));
                visuals.widgets.hovered.bg_fill = Color32::from_rgb(232, 239, 247);
                visuals.widgets.hovered.bg_stroke =
                    Stroke::new(1.0, Color32::from_rgb(133, 160, 190));
                visuals.widgets.active.bg_fill = Color32::from_rgb(210, 228, 247);
                visuals.widgets.active.bg_stroke =
                    Stroke::new(1.0, Color32::from_rgb(54, 105, 160));
            }
            for widget in [
                &mut visuals.widgets.noninteractive,
                &mut visuals.widgets.inactive,
                &mut visuals.widgets.hovered,
                &mut visuals.widgets.active,
            ] {
                widget.corner_radius = egui::CornerRadius::same(3);
            }
            style.visuals = visuals;
        });
        self.applied_theme = Some(theme);
    }

    fn theme_ui(&mut self, ui: &mut egui::Ui) {
        let mut preference = self.state.theme;
        egui::ComboBox::from_id_salt("theme_preference")
            .selected_text(preference.label())
            .show_ui(ui, |ui| {
                for option in [
                    ThemePreference::Auto,
                    ThemePreference::Light,
                    ThemePreference::Dark,
                ] {
                    ui.selectable_value(&mut preference, option, option.label());
                }
            });
        if preference != self.state.theme {
            self.state.theme = preference;
            ui.ctx().set_theme(preference.egui());
            if let Err(e) = self.state.save() {
                self.status = format!("Could not save theme preference: {e}");
            }
        }
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

    fn euroscope_ui(&mut self, ui: &mut egui::Ui) {
        match &self.euroscope {
            Some(d) => {
                ui.label(format!("Found: {}", d.path.display()));
                let version = d.version.as_deref().unwrap_or("unknown");
                ui.label(format!("Version: {version}"));
                if let Some(m) = &self.manifest {
                    let required = &m.euroscope.required_version;
                    if d.version
                        .as_deref()
                        .is_some_and(|v| euroscope::is_required_version(v, required))
                    {
                        ui.label("Version OK.");
                    } else {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!(
                                "Version {required} is required. Fully uninstall this version before installing it; downgrading requires a full uninstall."
                            ),
                        );
                        ui.label("After uninstalling, click Re-check, then install the required version.");
                        if ui.button("Open uninstall options").clicked() {
                            self.status = match euroscope::open_uninstaller() {
                                Ok(()) => {
                                    "Uninstall EuroScope in Installed apps, then click Re-check."
                                        .into()
                                }
                                Err(e) => format!("Could not open uninstall options: {e}"),
                            };
                        }
                    }
                }
            }
            None => {
                ui.colored_label(ui.visuals().error_fg_color, "EuroScope was not found.");
                ui.label("For a fresh install, follow the EuroScope setup guide.");
                ui.hyperlink_to("Open the EuroScope Setup Guide", euroscope::SETUP_GUIDE_URL);
                if ui
                    .add_enabled(self.job.is_none(), egui::Button::new("Install EuroScope"))
                    .clicked()
                {
                    if let Some(m) = &self.manifest {
                        let url = m.euroscope.download_url.clone();
                        let shared = download::Shared::default();
                        let worker = shared.clone();
                        let handle = std::thread::spawn(move || euroscope::install(&url, &worker));
                        self.job = Some(Job { shared, handle });
                        self.status.clear();
                    } else {
                        self.status = "Manifest unavailable".into();
                    }
                }
            }
        }
        self.progress_ui(ui);
        let ok = self.euroscope_ok();
        if self.recheck_ui(ui, "euroscope", ok) {
            self.euroscope = euroscope::detect();
        }
    }

    /// Re-check button with a brief spinner, then a tick/cross result. Returns true when the check should run.
    fn recheck_ui(&mut self, ui: &mut egui::Ui, key: &'static str, ok: bool) -> bool {
        const SPIN: std::time::Duration = std::time::Duration::from_millis(600);
        const SHOW: std::time::Duration = std::time::Duration::from_millis(3000);
        let mut run = false;
        let checking = matches!(self.rechecking, Some((k, t)) if k == key && t.elapsed() < SPIN);
        if matches!(self.rechecking, Some((k, t)) if k == key && t.elapsed() >= SPIN) {
            self.rechecking = None;
            self.checked = Some((key, std::time::Instant::now()));
            run = true;
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!checking, egui::Button::new("Re-check"))
                .clicked()
            {
                self.rechecking = Some((key, std::time::Instant::now()));
                self.checked = None;
            }
            if checking {
                ui.add(egui::Spinner::new());
                ui.label("Checking...");
                ui.ctx().request_repaint();
            } else if let Some((k, t)) = self.checked {
                if k == key && t.elapsed() < SHOW {
                    if ok {
                        ui.colored_label(
                            if ui.visuals().dark_mode {
                                egui::Color32::from_rgb(110, 190, 145)
                            } else {
                                egui::Color32::from_rgb(32, 126, 78)
                            },
                            "✔ Up to date",
                        );
                    } else {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            "✖ Not found / needs attention",
                        );
                    }
                    ui.ctx().request_repaint_after(SHOW);
                }
            }
        });
        run
    }

    fn vcredist_ui(&mut self, ui: &mut egui::Ui) {
        if self.vcredist {
            ui.label("Visual C++ Redistributable (x86) is installed.");
        } else {
            ui.colored_label(
                ui.visuals().error_fg_color,
                "Visual C++ Redistributable not found.",
            );
            ui.hyperlink_to("Microsoft's supported downloads", vcredist::DOWNLOADS_URL);
            if ui
                .add_enabled(self.job.is_none(), egui::Button::new("Install"))
                .clicked()
            {
                self.status = match self.manifest.as_ref() {
                    Some(m) => {
                        let url = m.vcredist_url.clone();
                        let shared = download::Shared::default();
                        let worker = shared.clone();
                        let handle = std::thread::spawn(move || vcredist::install(&url, &worker));
                        self.job = Some(Job { shared, handle });
                        String::new()
                    }
                    None => "Manifest unavailable".into(),
                };
            }
        }
        let ok = self.vcredist;
        if self.recheck_ui(ui, "vcredist", ok) {
            self.vcredist = vcredist::is_installed();
        }
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

    /// Start installing/updating the pack on a worker thread, optionally backing up and preserving the layout first.
    fn start_update(&mut self, opts: UpdateOptions) {
        if self.job.is_some() {
            return;
        }
        self.refresh_pack_version();
        let (Some(m), Some(dir)) = (&self.manifest, self.pack_dir()) else {
            self.status = "Update information unavailable".into();
            return;
        };
        let installed = self.state.pack_version;
        let Some(plan) = updater::plan(m, installed) else {
            self.status = "No releases found".into();
            return;
        };
        let installed_release =
            installed.and_then(|v| m.releases.iter().find(|r| r.version == v).cloned());
        if opts.save_layout && installed_release.is_none() {
            self.status =
                "Cannot save positions: installed pack version not found in the release list."
                    .into();
            return;
        }
        let cfg = if opts.apply_config {
            if !config::exists(Some(&dir)) {
                self.status =
                    "No saved settings to apply; configure them first or untick that option."
                        .into();
                return;
            }
            let c = config::load(Some(&dir));
            if let Some(msg) = c.validate() {
                self.status = format!("Saved settings are incomplete: {msg}");
                return;
            }
            Some(c)
        } else {
            None
        };
        let tag = installed.map(|v| v.to_string());
        let keep = self.state.backups_to_keep.unwrap_or(backup::DEFAULT_KEEP);
        let shared = download::Shared::default();
        let worker = shared.clone();
        let handle = std::thread::spawn(move || {
            // The reference download doesn't touch the pack, so it runs alongside the backup.
            let pristine = match (opts.save_layout, installed_release) {
                (true, Some(release)) => {
                    let w = worker.clone();
                    Some(std::thread::spawn(move || {
                        pack::fetch_pristine_asrs(&release, &w)
                    }))
                }
                _ => None,
            };
            if opts.backup {
                backup::create(&dir, tag.as_deref(), keep, &worker)?;
            }
            if let Some(h) = pristine {
                let pristine = h
                    .join()
                    .map_err(|_| anyhow::anyhow!("layout download panicked"))??;
                layout::save_changes(&dir, &pristine)?;
            }
            updater::execute(plan, &dir, &worker)?;
            match &cfg {
                // Also re-applies any saved screen layout.
                Some(cfg) => config::apply(cfg, &dir, &worker)?,
                None => {}
            }
            Ok(())
        });
        self.job = Some(Job { shared, handle });
        self.status.clear();
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

    fn pack_ui(&mut self, ui: &mut egui::Ui) {
        self.refresh_pack_version();
        match self.state.pack_version {
            Some(v) => ui.label(format!("Installed pack version: {v}")),
            None => ui.label("Controller pack not installed."),
        };
        if let Some(dir) = self.pack_dir() {
            ui.label(format!("Location: {}", dir.display()));
        }
        self.updates_ui(ui);
        self.progress_ui(ui);
    }

    fn updates_ui(&mut self, ui: &mut egui::Ui) {
        self.refresh_pack_version();
        let Some(m) = &self.manifest else {
            ui.label("Update information unavailable.");
            return;
        };
        let plan = updater::plan(m, self.state.pack_version);
        let mut start = false;
        let idle = self.job.is_none();
        let existing = self.state.pack_version.is_some();
        match &plan {
            None => {
                ui.label("No releases found.");
            }
            Some(updater::Plan::UpToDate) => {
                ui.label("You are up to date.");
            }
            Some(updater::Plan::Patches(p)) => {
                ui.label(format!("{} update(s) to apply, in order:", p.len()));
                for r in p {
                    ui.label(format!("  {}", r.version));
                }
                start = ui.add_enabled(idle, egui::Button::new("Update")).clicked();
            }
            Some(updater::Plan::Reinstall(r)) => {
                ui.label(format!("Full install of {}.", r.version));
                start = ui.add_enabled(idle, egui::Button::new("Install")).clicked();
            }
        }
        if start {
            if existing {
                self.confirm_update = true;
            } else {
                self.start_update(UpdateOptions::default());
            }
        }
        if self.confirm_update {
            self.update_prompt(ui);
        }
    }

    /// Modal asking the two questions that are easy to miss as tick boxes.
    fn update_prompt(&mut self, ui: &mut egui::Ui) {
        let dir = self.pack_dir();
        let settings_ok =
            config::exists(dir.as_deref()) && config::load(dir.as_deref()).validate().is_none();
        let saved_layout = layout::load_layout().len();
        let mut opts = self.update_opts;
        let (mut go, mut cancel) = (false, false);
        let yes_no = |ui: &mut egui::Ui, question: &str, value: &mut bool, enabled: bool| {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(question).strong().size(18.0));
            ui.horizontal(|ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    ui.selectable_value(value, true, "Yes");
                    ui.selectable_value(value, false, "No");
                });
            });
        };
        let modal = egui::Modal::new(egui::Id::new("update_prompt")).show(ui.ctx(), |ui| {
            ui.set_width(440.0);
            ui.heading("Before we update");
            yes_no(
                ui,
                "Back up your current pack first?",
                &mut opts.backup,
                true,
            );
            if !settings_ok {
                opts.apply_config = false;
            }
            yes_no(
                ui,
                "Apply your saved settings and screen layout once the update is done?",
                &mut opts.apply_config,
                settings_ok,
            );
            if !settings_ok {
                ui.weak("No complete saved settings were found, so there is nothing to apply.");
            }
            yes_no(
                ui,
                "Take a new snapshot of your current screen layout first?",
                &mut opts.save_layout,
                true,
            );
            ui.weak(if opts.save_layout {
                "Your current EuroScope window positions will replace the saved layout."
            } else if saved_layout > 0 {
                "No new snapshot: your previously saved layout will be re-applied as it is."
            } else {
                "No new snapshot, and no layout has been saved yet."
            });
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                go = ui
                    .add_enabled(
                        self.job.is_none(),
                        egui::Button::new(egui::RichText::new("Start update").strong())
                            .fill(ui.visuals().selection.bg_fill)
                            .min_size(egui::vec2(160.0, 38.0)),
                    )
                    .clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        self.update_opts = opts;
        if go {
            self.confirm_update = false;
            self.start_update(opts);
        } else if cancel || modal.should_close() {
            self.confirm_update = false;
        }
    }

    /// Browse, restore, delete and configure retention of pack backups.
    fn backups_ui(&mut self, ui: &mut egui::Ui) {
        if self.backups_dirty {
            self.backups = backup::list();
            self.backups_dirty = false;
            if !self
                .backups
                .iter()
                .any(|b| Some(&b.path) == self.selected_backup.as_ref().map(|(p, _)| p))
            {
                self.selected_backup = None;
            }
        }
        let idle = self.job.is_none();
        let mut keep = self.state.backups_to_keep.unwrap_or(backup::DEFAULT_KEEP);
        ui.horizontal(|ui| {
            ui.label("Backups to keep:");
            if ui
                .add(egui::DragValue::new(&mut keep).range(1..=50))
                .changed()
            {
                let mut s = State::load();
                s.backups_to_keep = Some(keep);
                self.status = match s.save().and_then(|_| backup::prune(keep)) {
                    Ok(()) => String::new(),
                    Err(e) => format!("Could not save setting: {e}"),
                };
                self.state.backups_to_keep = Some(keep);
                self.backups_dirty = true;
            }
            ui.label("Older backups are deleted automatically.");
        });
        if let Some(d) = backup::dir() {
            ui.label(format!("Stored in: {}", d.display()));
        }
        ui.separator();
        if self.backups.is_empty() {
            ui.label("No backups yet. One can be made before each pack update.");
            return;
        }
        let mut select = None;
        egui::ScrollArea::vertical()
            .id_salt("backup_list")
            .max_height(180.0)
            .show(ui, |ui| {
                for b in &self.backups {
                    let label = format!(
                        "{}  -  {}  ({:.1} MB)",
                        b.name(),
                        b.date(),
                        b.size as f64 / 1_048_576.0
                    );
                    let on = self
                        .selected_backup
                        .as_ref()
                        .is_some_and(|(p, _)| *p == b.path);
                    if ui.selectable_label(on, label).clicked() {
                        select = Some(b.clone());
                    }
                }
            });
        if let Some(b) = select {
            let files = backup::contents(&b).map_err(|e| e.to_string());
            self.selected_backup = Some((b.path.clone(), files));
        }
        let Some((path, files)) = &self.selected_backup else {
            return;
        };
        let Some(b) = self.backups.iter().find(|b| &b.path == path).cloned() else {
            return;
        };
        ui.separator();
        ui.label(format!("Contents of {} ({}):", b.name(), b.date()));
        match files {
            Ok(f) => {
                ui.label(format!("{} file(s)", f.len()));
                egui::ScrollArea::vertical()
                    .id_salt("backup_files")
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for name in f {
                            ui.monospace(name);
                        }
                    });
            }
            Err(e) => {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
        }
        ui.separator();
        let mut restore = false;
        ui.horizontal(|ui| {
            restore = ui
                .add_enabled(idle, egui::Button::new("Restore this backup"))
                .clicked();
            if ui.add_enabled(idle, egui::Button::new("Delete")).clicked() {
                self.status = match backup::delete(&b) {
                    Ok(()) => "Backup deleted.".into(),
                    Err(e) => format!("Could not delete backup: {e}"),
                };
                self.backups_dirty = true;
            }
        });
        ui.label("Restoring replaces your current pack folder. Close EuroScope first.");
        self.progress_ui(ui);
        if restore {
            let Some(dir) = self.pack_dir() else {
                self.status = "Pack location unknown.".into();
                return;
            };
            let shared = download::Shared::default();
            let worker = shared.clone();
            let handle = std::thread::spawn(move || backup::restore(&b, &dir, &worker));
            self.job = Some(Job { shared, handle });
            self.status.clear();
        }
    }

    fn yes_no(ui: &mut egui::Ui, label: &str, value: &mut String) {
        let mut on = value == "y";
        if ui.checkbox(&mut on, label).changed() {
            *value = if on { "y" } else { "n" }.into();
        }
    }

    fn choice_ui(
        ui: &mut egui::Ui,
        textures: &mut std::collections::HashMap<String, egui::TextureHandle>,
        kind: &str,
        title: &str,
        options: &[(&str, &str, &str)],
        value: &mut String,
    ) {
        // Stored colours are BGR (Windows COLORREF).
        let swatch = |ui: &mut egui::Ui, color: &str| {
            let c = color.parse::<u32>().unwrap_or(0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            ui.painter().rect_filled(
                rect,
                2.0,
                egui::Color32::from_rgb(c as u8, (c >> 8) as u8, (c >> 16) as u8),
            );
            ui.painter().rect_stroke(
                rect,
                2.0,
                egui::Stroke::new(1.0, egui::Color32::from_gray(190)),
                egui::StrokeKind::Inside,
            );
        };
        for (key, _, _) in options {
            let id = format!("{kind}{key}");
            if textures.contains_key(&id) {
                continue;
            }
            if let Some(tex) = config::preview_bytes(kind, key)
                .and_then(|b| image::load_from_memory(b).ok())
                .map(|img| {
                    let img = img.into_rgba8();
                    let size = [img.width() as usize, img.height() as usize];
                    ui.ctx().load_texture(
                        id.clone(),
                        egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()),
                        egui::TextureOptions::LINEAR,
                    )
                })
            {
                textures.insert(id, tex);
            }
        }
        ui.label(title);
        ui.horizontal(|ui| {
            let current = options
                .iter()
                .find(|(k, _, _)| *k == value.as_str())
                .unwrap_or(&options[0]);
            swatch(ui, current.2);
            egui::ComboBox::from_id_salt(kind)
                .width(380.0)
                .selected_text(current.1)
                .show_ui(ui, |ui| {
                    for (key, desc, color) in options {
                        ui.horizontal(|ui| {
                            swatch(ui, color);
                            let r = ui.selectable_value(value, key.to_string(), *desc);
                            if let Some(tex) = textures.get(&format!("{kind}{key}")) {
                                r.on_hover_ui(|ui| {
                                    ui.add(egui::Image::new(tex).max_width(320.0));
                                });
                            }
                        });
                    }
                });
        });
        ui.end_row();
    }

    fn config_ui(&mut self, ui: &mut egui::Ui) {
        if self.capturing_asel {
            ui.ctx().request_repaint();
            if let Some(vk) = config::pressed_vk() {
                self.capturing_asel = false;
                if vk != 0x1B {
                    match config::asel_from_vk(vk) {
                        Some(code) => self.config.asel_key = code,
                        None => {
                            self.status =
                                "Could not map that key; keeping the previous bind.".into()
                        }
                    }
                }
            }
        }
        let Some(dir) = self.pack_dir() else {
            ui.label("Pack location unknown.");
            return;
        };
        if !dir.is_dir() {
            ui.label("Install the controller pack first.");
            return;
        }
        let from_pack =
            config::local_path().is_some_and(|p| !p.exists()) && config::exists(Some(&dir));
        if from_pack {
            ui.label(
                "Loaded your existing settings from the pack; saving will store them locally.",
            );
        }
        let problem = self.config.validate();
        ui.horizontal(|ui| {
            for (tab, name) in ConfigTab::ALL {
                let label = if tab == ConfigTab::Details && problem.is_some() {
                    egui::RichText::new(format!("{name} ●")).color(ui.visuals().error_fg_color)
                } else {
                    egui::RichText::new(name)
                };
                ui.selectable_value(&mut self.config_tab, tab, label.size(17.0));
                ui.add_space(8.0);
            }
        });
        ui.separator();
        // Leave room below the content so the action bar is always on screen.
        let scroll_height = (ui.available_height() - 100.0).max(120.0);
        egui::ScrollArea::vertical().max_height(scroll_height).auto_shrink([false, false]).show(ui, |ui| {
            let c = &mut self.config;
            let hint = |ui: &mut egui::Ui, bad: bool, msg: &str| {
                if bad {
                    ui.colored_label(ui.visuals().error_fg_color, msg);
                }
            };
            let font = config::FONT_OPTIONS.iter().find(|o| o.0 == c.font_size).map_or("", |o| o.1);
            let rdf = config::RDF_OPTIONS.iter().find(|o| o.0 == c.rdf_mode).map_or("", |o| o.1);
            let saved = layout::load_layout().len();
            ui.add_space(6.0);

            match self.config_tab {
                ConfigTab::Details => {
            egui::Grid::new("basic").num_columns(3).spacing([16.0, 12.0]).show(ui, |ui| {
                ui.label("Name (as on VATSIM)");
                ui.add_sized([360.0, 30.0], egui::TextEdit::singleline(&mut c.name));
                hint(ui, c.name.trim().is_empty(), "Required");
                ui.end_row();
                ui.label("Initials");
                ui.add_sized([360.0, 30.0], egui::TextEdit::singleline(&mut c.initials).hint_text("2-3 letters"));
                hint(ui, !(2..=3).contains(&c.initials.trim().chars().count()), "2-3 letters");
                ui.end_row();
                ui.label("VATSIM CID");
                ui.add_sized([360.0, 30.0], egui::TextEdit::singleline(&mut c.cid));
                hint(ui, !config::is_valid_cid(&c.cid), "6 or 7 digits");
                ui.end_row();
                ui.label("Rating");
                let idx = c.rating.parse::<usize>().unwrap_or(0).min(config::RATINGS.len() - 1);
                egui::ComboBox::from_id_salt("rating")
                    .selected_text(config::RATINGS[idx])
                    .show_ui(ui, |ui| {
                        for (i, r) in config::RATINGS.iter().enumerate() {
                            ui.selectable_value(&mut c.rating, i.to_string(), *r);
                        }
                    });
                ui.end_row();
                ui.label("VATSIM password");
                ui.add_sized([360.0, 30.0], egui::TextEdit::singleline(&mut c.password).password(true));
                hint(ui, c.password.is_empty(), "Required");
                ui.end_row();
                ui.label("Hoppie CPDLC code");
                ui.add_sized([360.0, 30.0], egui::TextEdit::singleline(&mut c.cpdlc).hint_text("optional"));
                ui.end_row();
            });
                }

                ConfigTab::Appearance => {
                    egui::Grid::new("appearance").num_columns(2).spacing([16.0, 12.0]).show(ui, |ui| {
                        ui.label("Text size");
                        egui::ComboBox::from_id_salt("font_size").selected_text(font).show_ui(ui, |ui| {
                            for (key, label, _) in config::FONT_OPTIONS {
                                ui.selectable_value(&mut c.font_size, key.to_string(), *label);
                            }
                        });
                        ui.end_row();
                        Self::choice_ui(ui, &mut self.textures, "coastline", "Coastline colour", config::COAST_OPTIONS, &mut c.coast_choice);
                        Self::choice_ui(ui, &mut self.textures, "land", "Land colour", config::LAND_OPTIONS, &mut c.land_choice);
                    });
                    ui.add_space(8.0);
                    ui.weak("Text size applies to metar, chat and list headers. Open a colour list and hover an entry to preview it.");
                }

                ConfigTab::Controlling => {
                    Self::yes_no(ui, "Realistic datablocks for LAC/LTC (no climb/descent arrows)", &mut c.realistic_tags);
                    Self::yes_no(ui, "Realistic code/callsign conversion", &mut c.realistic_conversion);
                    Self::yes_no(ui, "DiscordEuroscope plugin (shows where you're controlling)", &mut c.discord_presence);
                    ui.horizontal(|ui| {
                        ui.label("RDF (radio direction finding)");
                        egui::ComboBox::from_id_salt("rdf").selected_text(rdf).show_ui(ui, |ui| {
                            for (key, label) in config::RDF_OPTIONS {
                                ui.selectable_value(&mut c.rdf_mode, key.to_string(), *label);
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        let bound = if c.asel_key.is_empty() {
                            "default (NUMPLUS)".to_string()
                        } else {
                            config::asel_name(&c.asel_key).unwrap_or_else(|| format!("custom (code {})", c.asel_key))
                        };
                        ui.label(format!("ASEL key: {bound}"));
                        if self.capturing_asel {
                            ui.label("Press a key (Esc to cancel)...");
                        } else if ui.button("Set").clicked() {
                            self.capturing_asel = true;
                        }
                        if !c.asel_key.is_empty() && ui.button("Reset").clicked() {
                            c.asel_key.clear();
                        }
                    });
                }

                ConfigTab::Layout => {
                    ui.weak("Move your windows in EuroScope and save your ASRs, then save your layout. It is re-applied whenever you apply configuration.");
                    ui.horizontal(|ui| {
                        let idle = self.job.is_none();
                        if ui.add_enabled(idle, egui::Button::new("Save current layout")).clicked() {
                            let release = self.state.pack_version.and_then(|v| {
                                self.manifest.as_ref()?.releases.iter().find(|r| r.version == v).cloned()
                            });
                            match release {
                                Some(release) => {
                                    let (pack, shared) = (dir.clone(), download::Shared::default());
                                    let worker = shared.clone();
                                    let handle = std::thread::spawn(move || {
                                        let pristine = pack::fetch_pristine_asrs(&release, &worker)?;
                                        layout::save_changes(&pack, &pristine).map(|_| ())
                                    });
                                    self.job = Some(Job { shared, handle });
                                    self.status.clear();
                                }
                                None => {
                                    self.status = "Installed pack version not found in the release list (offline?).".into()
                                }
                            }
                        }
                        if ui.add_enabled(saved > 0 && idle, egui::Button::new("Clear saved layout")).clicked() {
                            self.status = match layout::clear_layout() {
                                Ok(()) => "Saved layout cleared.".into(),
                                Err(e) => format!("Could not clear layout: {e}"),
                            };
                        }
                    });
                    ui.label(if saved > 0 { format!("{saved} ASR file(s) with saved changes") } else { "No layout saved yet.".into() });
                }
            }
        });
        ui.add_space(6.0);
        self.progress_ui(ui);
        let idle = self.job.is_none();
        ui.horizontal(|ui| {
            let button =
                egui::Button::new(egui::RichText::new("Save and apply").size(18.0).strong())
                    .fill(ui.visuals().selection.bg_fill)
                    .min_size(egui::vec2(200.0, 40.0));
            if ui.add_enabled(idle && problem.is_none(), button).clicked() {
                let cfg = self.config.clone();
                let shared = download::Shared::default();
                let worker = shared.clone();
                let handle = std::thread::spawn(move || {
                    config::save(&cfg)?;
                    config::apply(&cfg, &dir, &worker)
                });
                self.job = Some(Job { shared, handle });
                self.status.clear();
            }
            if let Some(msg) = problem {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("Complete your details: {msg}"),
                );
            }
        });
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
                        self.config_ui(ui);
                    }
                    Step::Backups => {
                        self.backups_ui(ui);
                    }
                });
            });
    }
}
