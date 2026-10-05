//! UI: a sidebar of wizard steps plus a content panel.

use crate::{euroscope, manifest::Manifest, state::State, updater, vcredist};
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
        };
        app.refresh();
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
                    if !euroscope::meets_minimum(v, &m.euroscope.minimum_version) {
                        ui.colored_label(egui::Color32::YELLOW, "Version is older than required.");
                    }
                }
            }
            None => {
                ui.colored_label(egui::Color32::LIGHT_RED, "EuroScope was not found.");
                if ui.button("Install EuroScope").clicked() {
                    self.status = match self.manifest.as_ref() {
                        Some(m) => euroscope::install(&m.euroscope.download_url)
                            .err()
                            .map(|e| e.to_string())
                            .unwrap_or_default(),
                        None => "Manifest unavailable".into(),
                    };
                }
            }
        }
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

    fn pack_ui(&mut self, ui: &mut egui::Ui) {
        match self.state.pack_version {
            Some(v) => ui.label(format!("Installed pack version: {v}")),
            None => ui.label("Controller pack not installed."),
        };
        if ui.button("Install controller pack").clicked() {
            self.status = "Not implemented yet".into();
        }
    }

    fn updates_ui(&mut self, ui: &mut egui::Ui) {
        let Some(m) = &self.manifest else {
            ui.label("Update information unavailable.");
            return;
        };
        ui.label(format!("Latest version: {}", m.latest_version()));
        match updater::plan(m, self.state.pack_version) {
            updater::Plan::UpToDate => {
                ui.label("You are up to date.");
            }
            updater::Plan::Patches(p) => {
                ui.label(format!("{} patch(es) to apply, in order:", p.len()));
                for patch in &p {
                    ui.label(format!("  v{} -> v{}", patch.from, patch.version));
                }
                let _ = ui.button("Update");
            }
            updater::Plan::Reinstall(p) => {
                ui.label(format!("Full reinstall then {} patch(es).", p.len()));
                let _ = ui.button("Repair");
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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
