//! UI: a status-aware sidebar (EuroScope, VC++, pack, configuration, backups) plus a content panel.

mod backups_ui;
mod config_ui;
mod euroscope_ui;
mod panels;
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
