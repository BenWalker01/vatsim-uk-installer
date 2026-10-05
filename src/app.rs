//! UI: a sidebar of wizard steps plus a content panel.

use crate::{download, euroscope, manifest::Manifest, pack, state::State, updater, vcredist};
use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Welcome,
    EuroScope,
    VcRedist,
    ControllerPack,
    Updates,
    Done,
}

const STEPS: &[(Step, &str)] = &[
    (Step::Welcome, "Welcome"),
    (Step::EuroScope, "EuroScope"),
    (Step::VcRedist, "VC++ Redistributable"),
    (Step::ControllerPack, "Controller Pack"),
    (Step::Updates, "Updates"),
    (Step::Done, "Finish"),
];

pub struct App {
    step: Step,
    state: State,
    manifest: Option<Manifest>,
    manifest_error: Option<String>,
    euroscope: Option<euroscope::Detected>,
    vcredist: bool,
    status: String,
    job: Option<Job>,
}

struct Job {
    shared: download::Shared,
    handle: std::thread::JoinHandle<anyhow::Result<()>>,
}

impl App {
    pub fn new() -> Self {
        let mut app = App {
            step: Step::Welcome,
            state: State::load(),
            manifest: None,
            manifest_error: None,
            euroscope: None,
            vcredist: false,
            status: String::new(),
            job: None,
        };
        app.refresh();
        app.reload_state();
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

    fn next(&mut self) {
        if let Some(i) = STEPS.iter().position(|(s, _)| *s == self.step) {
            if let Some((s, _)) = STEPS.get(i + 1) {
                self.step = *s;
            }
        }
    }

    fn euroscope_ui(&mut self, ui: &mut egui::Ui) {
        match &self.euroscope {
            Some(d) => {
                ui.label(format!("Found: {}", d.path.display()));
                let version = d.version.as_deref().unwrap_or("unknown");
                ui.label(format!("Version: {version}"));
                if let (Some(v), Some(m)) = (&d.version, &self.manifest) {
                    let required = &m.euroscope.required_version;
                    if euroscope::is_required_version(v, required) {
                        ui.label("Version OK.");
                    } else {
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            format!("Version must be exactly {required}; install it to continue."),
                        );
                    }
                }
            }
            None => {
                ui.colored_label(egui::Color32::LIGHT_RED, "EuroScope was not found.");
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
        if ui.button("Re-check").clicked() {
            self.euroscope = euroscope::detect();
        }
    }

    fn vcredist_ui(&mut self, ui: &mut egui::Ui) {
        if self.vcredist {
            ui.label("Visual C++ Redistributable (x86) is installed.");
        } else {
            ui.colored_label(egui::Color32::LIGHT_RED, "Visual C++ Redistributable not found.");
            if ui.button("Install").clicked() {
                self.status = match self.manifest.as_ref() {
                    Some(m) => vcredist::install(&m.vcredist_url)
                        .err()
                        .map(|e| e.to_string())
                        .unwrap_or_default(),
                    None => "Manifest unavailable".into(),
                };
            }
        }
        if ui.button("Re-check").clicked() {
            self.vcredist = vcredist::is_installed();
        }
    }

    /// Reload state, preferring the pack's own `version.txt` over our record.
    fn reload_state(&mut self) {
        self.state = State::load();
        if let Some(v) = self.pack_dir().and_then(|d| pack::installed_version(&d)) {
            self.state.pack_version = Some(v);
        }
    }

    fn pack_dir(&self) -> Option<std::path::PathBuf> {
        self.state.pack_dir.clone().or_else(pack::default_dir)
    }

    /// Start installing/updating the pack on a worker thread.
    fn start_update(&mut self) {
        let (Some(m), Some(dir)) = (&self.manifest, self.pack_dir()) else {
            self.status = "Update information unavailable".into();
            return;
        };
        let Some(plan) = updater::plan(m, self.state.pack_version) else {
            self.status = "No releases found".into();
            return;
        };
        let shared = download::Shared::default();
        let worker = shared.clone();
        let handle = std::thread::spawn(move || updater::execute(plan, &dir, &worker));
        self.job = Some(Job { shared, handle });
        self.status.clear();
    }

    /// Show progress for a running job and collect its result once finished.
    fn poll_job(&mut self, ui: &mut egui::Ui) {
        let Some(job) = &self.job else { return };
        if job.handle.is_finished() {
            let job = self.job.take().unwrap();
            self.status = match job.handle.join() {
                Ok(Ok(())) => "Done".into(),
                Ok(Err(e)) => format!("Failed: {e}"),
                Err(_) => "Worker thread panicked".into(),
            };
            self.reload_state();
            self.euroscope = euroscope::detect();
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
        match self.state.pack_version {
            Some(v) => ui.label(format!("Installed pack version: {v}")),
            None => ui.label("Controller pack not installed."),
        };
        if let Some(dir) = self.pack_dir() {
            ui.label(format!("Location: {}", dir.display()));
        }
        self.progress_ui(ui);
        let idle = self.job.is_none();
        if self.state.pack_version.is_none()
            && ui.add_enabled(idle, egui::Button::new("Install controller pack")).clicked()
        {
            self.start_update();
        }
    }

    fn updates_ui(&mut self, ui: &mut egui::Ui) {
        let Some(m) = &self.manifest else {
            ui.label("Update information unavailable.");
            return;
        };
        let plan = updater::plan(m, self.state.pack_version);
        let mut start = false;
        let idle = self.job.is_none();
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
        self.progress_ui(ui);
        if start {
            self.start_update();
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_job(ui);
        egui::Panel::left("steps").show(ui, |ui| {
            ui.heading("Setup");
            for (s, name) in STEPS {
                if ui.selectable_label(self.step == *s, *name).clicked() {
                    self.step = *s;
                }
            }
        });

        egui::Panel::bottom("footer").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.step != Step::Done && ui.button("Next").clicked() {
                        self.next();
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            match self.step {
                Step::Welcome => {
                    ui.heading("VATSIM UK Installer");
                    ui.label("Sets up EuroScope and the UK controller pack.");
                    if let Some(e) = &self.manifest_error {
                        ui.colored_label(egui::Color32::YELLOW, format!("Offline: {e}"));
                    }
                }
                Step::EuroScope => {
                    ui.heading("EuroScope");
                    self.euroscope_ui(ui);
                }
                Step::VcRedist => {
                    ui.heading("Visual C++ Redistributable");
                    self.vcredist_ui(ui);
                }
                Step::ControllerPack => {
                    ui.heading("Controller Pack");
                    self.pack_ui(ui);
                }
                Step::Updates => {
                    ui.heading("Updates");
                    self.updates_ui(ui);
                }
                Step::Done => {
                    ui.heading("All done");
                }
            }
        });
    }
}
