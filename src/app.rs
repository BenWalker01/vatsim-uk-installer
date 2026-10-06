//! UI: a sidebar of wizard steps plus a content panel.

use crate::{
    config::{self, Config},
    download, euroscope, layout,
    manifest::Manifest,
    pack,
    state::State,
    updater, vcredist,
};
use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Welcome,
    EuroScope,
    VcRedist,
    ControllerPack,
    Updates,
    Configure,
    Done,
}

const STEPS: &[(Step, &str)] = &[
    (Step::Welcome, "Welcome"),
    (Step::EuroScope, "EuroScope"),
    (Step::VcRedist, "VC++ Redistributable"),
    (Step::ControllerPack, "Controller Pack"),
    (Step::Updates, "Updates"),
    (Step::Configure, "Configuration"),
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
    config: Config,
    capturing_asel: bool,
    textures: std::collections::HashMap<String, egui::TextureHandle>,
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
            config: Config::default(),
            capturing_asel: false,
            textures: Default::default(),
        };
        app.refresh();
        app.reload_state();
        app.config = config::load(app.pack_dir().as_deref());
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
                if let Some(m) = &self.manifest {
                    let required = &m.euroscope.required_version;
                    if d.version
                        .as_deref()
                        .is_some_and(|v| euroscope::is_required_version(v, required))
                    {
                        ui.label("Version OK.");
                    } else {
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            format!(
                                "Version {required} is required. Fully uninstall this version before installing it; downgrading requires a full uninstall."
                            ),
                        );
                        ui.label("After uninstalling, click Re-check, then install the required version.");
                        if ui.button("Open uninstall options").clicked() {
                            self.status = match euroscope::open_uninstaller() {
                                Ok(()) => {
                                    "Uninstall EuroScope in Installed apps, then click Re-check.".into()
                                }
                                Err(e) => format!("Could not open uninstall options: {e}"),
                            };
                        }
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

    /// Start installing/updating the pack on a worker thread.
    fn start_update(&mut self) {
        self.refresh_pack_version();
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
        self.refresh_pack_version();
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
        self.refresh_pack_version();
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
        ui.label(title);
        for (key, desc, color) in options {
            ui.horizontal(|ui| {
                let c = color.parse::<u32>().unwrap_or(0);
                // Stored colours are BGR (Windows COLORREF).
                let swatch = egui::Color32::from_rgb(c as u8, (c >> 8) as u8, (c >> 16) as u8);
                let (rect, swatch_resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, swatch);
                let radio = ui.radio_value(value, key.to_string(), *desc);
                let id = format!("{kind}{key}");
                if !textures.contains_key(&id) {
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
                        textures.insert(id.clone(), tex);
                    }
                }
                if let Some(tex) = textures.get(&id) {
                    let tip = |ui: &mut egui::Ui| {
                        ui.add(egui::Image::new(tex).max_width(320.0));
                    };
                    swatch_resp.on_hover_ui(tip);
                    radio.on_hover_ui(tip);
                }
            });
        }
    }

    fn config_ui(&mut self, ui: &mut egui::Ui) {
        if self.capturing_asel {
            ui.ctx().request_repaint();
            if let Some(vk) = config::pressed_vk() {
                self.capturing_asel = false;
                if vk != 0x1B {
                    match config::asel_from_vk(vk) {
                        Some(code) => self.config.asel_key = code,
                        None => self.status = "Could not map that key; keeping the previous bind.".into(),
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
        let from_pack = config::local_path().is_some_and(|p| !p.exists()) && config::exists(Some(&dir));
        if from_pack {
            ui.label("Loaded your existing settings from the pack; saving will store them locally.");
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            let c = &mut self.config;
            egui::Grid::new("basic").num_columns(2).show(ui, |ui| {
                ui.label("Name (as on VATSIM)");
                ui.text_edit_singleline(&mut c.name);
                ui.end_row();
                ui.label("Initials (2-3 letters)");
                ui.text_edit_singleline(&mut c.initials);
                ui.end_row();
                ui.label("VATSIM CID");
                ui.text_edit_singleline(&mut c.cid);
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
                ui.add(egui::TextEdit::singleline(&mut c.password).password(true));
                ui.end_row();
                ui.label("Hoppie CPDLC code");
                ui.text_edit_singleline(&mut c.cpdlc);
                ui.end_row();
            });
            Self::yes_no(ui, "Enable DiscordEuroscope plugin (shows where you're controlling)", &mut c.discord_presence);
            ui.label("Text size (metar, chat and list headers)");
            ui.horizontal(|ui| {
                for (key, label, _) in config::FONT_OPTIONS {
                    ui.radio_value(&mut c.font_size, key.to_string(), *label);
                }
            });
            ui.separator();
            ui.label("Screen layout");
            ui.label(
                "Move your windows in EuroScope and save your ASRs, then save your layout. It is compared \
                 against a fresh copy of your installed pack version and re-applied whenever you apply configuration.",
            );
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
                let saved = layout::load_layout().len();
                if ui.add_enabled(saved > 0, egui::Button::new("Clear saved layout")).clicked() {
                    self.status = match layout::clear_layout() {
                        Ok(()) => "Saved layout cleared.".into(),
                        Err(e) => format!("Could not clear layout: {e}"),
                    };
                }
                ui.label(format!("{saved} ASR file(s) with saved changes"));
            });
            ui.separator();
            ui.checkbox(&mut c.advanced_config, "Configure advanced options");
            if c.advanced_config {
                Self::yes_no(ui, "Realistic datablocks for LAC/LTC (no climb/descent arrows)", &mut c.realistic_tags);
                Self::yes_no(ui, "Realistic code/callsign conversion", &mut c.realistic_conversion);
                Self::choice_ui(ui, &mut self.textures, "coastline", "Coastline colour", config::COAST_OPTIONS, &mut c.coast_choice);
                Self::choice_ui(ui, &mut self.textures, "land", "Land colour", config::LAND_OPTIONS, &mut c.land_choice);
                ui.label("RDF (radio direction finding)");
                for (key, label) in config::RDF_OPTIONS {
                    ui.radio_value(&mut c.rdf_mode, key.to_string(), *label);
                }
                ui.horizontal(|ui| {
                    let bound = if c.asel_key.is_empty() {
                        "default (NUMPLUS)".to_string()
                    } else {
                        config::asel_name(&c.asel_key).unwrap_or_else(|| format!("custom (code {})", c.asel_key))
                    };
                    ui.label(format!("ASEL key: {bound}"));
                    if self.capturing_asel {
                        ui.label("Press a key (Esc to cancel)...");
                    } else if ui.button("Set ASEL key").clicked() {
                        self.capturing_asel = true;
                    }
                    if !c.asel_key.is_empty() && ui.button("Reset").clicked() {
                        c.asel_key.clear();
                    }
                });
            }
        });
        ui.separator();
        self.progress_ui(ui);
        let idle = self.job.is_none();
        if ui.add_enabled(idle, egui::Button::new("Save and apply")).clicked() {
            match self.config.validate() {
                Some(msg) => self.status = msg.to_string(),
                None => {
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
            }
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
                Step::Configure => {
                    ui.heading("Configuration");
                    self.config_ui(ui);
                }
                Step::Done => {
                    ui.heading("All done");
                }
            }
        });
    }
}
