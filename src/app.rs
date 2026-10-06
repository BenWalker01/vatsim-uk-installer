//! UI: a sidebar of wizard steps plus a content panel.

use crate::{
    backup,
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
    Backups,
    Done,
}

const STEPS: &[(Step, &str)] = &[
    (Step::Welcome, "Welcome"),
    (Step::EuroScope, "EuroScope"),
    (Step::VcRedist, "VC++ Runtime"),
    (Step::ControllerPack, "Controller Pack"),
    (Step::Updates, "Updates"),
    (Step::Configure, "Settings"),
    (Step::Backups, "Backups"),
    (Step::Done, "Finish"),
];

#[derive(Clone, Copy, Default)]
struct UpdateOptions {
    backup: bool,
    save_layout: bool,
    apply_config: bool,
}

pub struct App {
    /// Guided mode: sidebar navigation is locked until setup has been completed.
    wizard: bool,
    confirm_update: Option<UpdateOptions>,
    backups: Vec<backup::Backup>,
    backups_dirty: bool,
    selected_backup: Option<(std::path::PathBuf, Result<Vec<String>, String>)>,
    step: Step,
    state: State,
    manifest: Option<Manifest>,
    manifest_error: Option<String>,
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
            Step::Welcome => "Installer for the UK Controller Pack",
            Step::EuroScope => {
                "Install Euroscope version 3.2.3.2, or check what you already have installed"
            }
            Step::VcRedist => "The UK Controller Plugin needs this runtime",
            Step::ControllerPack => "Install a fresh controller pack",
            Step::Updates => "Update your pack to the latest version",
            Step::Configure => "Set or load settings",
            Step::Backups => "Restore a saved copy of your controller pack",
            Step::Done => "Happy Controlling!",
        }
    }

    fn setup_status(&self, step: Step) -> (&'static str, egui::Color32) {
        let ready = match step {
            Step::EuroScope => self.euroscope_ok(),
            Step::VcRedist => self.vcredist,
            Step::ControllerPack => self.state.pack_version.is_some(),
            _ => false,
        };
        if ready {
            ("Ready", egui::Color32::from_rgb(110, 190, 145))
        } else {
            ("Not set up", egui::Color32::from_rgb(158, 165, 174))
        }
    }

    pub fn new() -> Self {
        let mut app = App {
            wizard: true,
            confirm_update: None,
            backups: Vec::new(),
            backups_dirty: true,
            selected_backup: None,
            step: Step::Welcome,
            state: State::load(),
            manifest: None,
            manifest_error: None,
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
        app.wizard = !app.setup_complete();
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

    /// Steps shown in the sidebar; the Backups page only appears once setup is complete.
    fn steps(&self) -> Vec<(Step, &'static str)> {
        STEPS
            .iter()
            .copied()
            .filter(|(s, _)| !(self.wizard && *s == Step::Backups))
            .collect()
    }

    fn next(&mut self) {
        let steps = self.steps();
        if let Some(i) = steps.iter().position(|(s, _)| *s == self.step) {
            if let Some((s, _)) = steps.get(i + 1) {
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
                        ui.colored_label(egui::Color32::from_rgb(110, 190, 145), "✔ Up to date");
                    } else {
                        ui.colored_label(egui::Color32::LIGHT_RED, "✖ Not found / needs attention");
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
                egui::Color32::LIGHT_RED,
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

    /// Whether everything the wizard sets up is in place.
    fn setup_complete(&self) -> bool {
        self.euroscope_ok() && self.vcredist && self.state.pack_version.is_some()
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

    /// Whether the current wizard step is satisfied so the user may continue.
    fn step_satisfied(&self) -> bool {
        match self.step {
            Step::EuroScope => self.euroscope_ok(),
            Step::VcRedist => self.vcredist,
            Step::ControllerPack => self.state.pack_version.is_some(),
            _ => true,
        }
    }

    /// Start installing/updating the pack on a worker thread, optionally backing up and preserving the layout first.
    fn start_update(&mut self, opts: UpdateOptions) {
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
                None if opts.save_layout => {
                    layout::apply(&dir)?;
                }
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
            self.status = match job.handle.join() {
                Ok(Ok(())) => "Done".into(),
                Ok(Err(e)) => format!("Failed: {e}"),
                Err(_) => "Worker thread panicked".into(),
            };
            self.reload_state();
            self.backups_dirty = true;
            self.euroscope = euroscope::detect();
            self.vcredist = vcredist::is_installed();
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
            && ui
                .add_enabled(idle, egui::Button::new("Install controller pack"))
                .clicked()
        {
            self.start_update(UpdateOptions::default());
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
            if self.state.pack_version.is_some() {
                self.confirm_update = Some(UpdateOptions {
                    backup: true,
                    save_layout: true,
                    apply_config: config::exists(self.pack_dir().as_deref()),
                });
            } else {
                self.start_update(UpdateOptions::default());
            }
        }
        if let Some(mut opts) = self.confirm_update {
            ui.separator();
            ui.label("Before updating your pack:");
            ui.checkbox(&mut opts.backup, "Back up my existing pack first");
            ui.checkbox(
                &mut opts.save_layout,
                "Save the position of my items on screen and re-apply them after updating",
            );
            ui.checkbox(
                &mut opts.apply_config,
                "Re-apply my previous settings (name, tags, colours, etc.) after updating",
            );
            let (mut go, mut cancel) = (false, false);
            ui.horizontal(|ui| {
                go = ui.button("Continue").clicked();
                cancel = ui.button("Cancel").clicked();
            });
            if go {
                self.confirm_update = None;
                self.start_update(opts);
            } else if cancel {
                self.confirm_update = None;
            } else {
                self.confirm_update = Some(opts);
            }
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
                ui.colored_label(egui::Color32::LIGHT_RED, e);
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
                    egui::RichText::new(format!("{name} ●")).color(egui::Color32::LIGHT_RED)
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
                    ui.colored_label(egui::Color32::LIGHT_RED, msg);
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
                        if ui.add_enabled(saved > 0, egui::Button::new("Clear saved layout")).clicked() {
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
                    egui::Color32::LIGHT_RED,
                    format!("Complete your details: {msg}"),
                );
            }
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_job(ui);
        egui::Panel::left("steps")
            .resizable(false)
            .default_size(232.0)
            .size_range(232.0..=232.0)
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(30, 34, 40))
                    .inner_margin(egui::Margin::same(20)),
            )
            .show(ui, |ui| {
                ui.heading(egui::RichText::new("VATSIM UK").strong().size(20.0));
                ui.label(
                    egui::RichText::new("CONTROLLER PACK")
                        .small()
                        .color(egui::Color32::from_rgb(150, 160, 173)),
                );
                ui.add_space(18.0);
                ui.label(
                    egui::RichText::new(if self.wizard {
                        "SETUP"
                    } else {
                        "YOUR INSTALLATION"
                    })
                    .small()
                    .strong()
                    .color(egui::Color32::from_rgb(150, 160, 173)),
                );
                ui.add_space(8.0);
                let steps = self.steps();
                let current = steps.iter().position(|(s, _)| *s == self.step).unwrap_or(0);
                for (i, (s, name)) in steps.iter().enumerate() {
                    let active = self.step == *s;
                    let completed = self.wizard && i < current;
                    let text = if completed {
                        egui::RichText::new(format!("✓  {name}"))
                            .color(egui::Color32::from_rgb(110, 190, 145))
                    } else if active {
                        egui::RichText::new(format!("{:02}  {name}", i + 1))
                            .color(egui::Color32::from_rgb(154, 195, 235))
                            .strong()
                    } else {
                        egui::RichText::new(format!("{:02}  {name}", i + 1))
                            .color(egui::Color32::from_rgb(184, 190, 199))
                    };
                    if self.wizard {
                        egui::Frame::new()
                            .fill(if active {
                                egui::Color32::from_rgb(43, 54, 67)
                            } else {
                                egui::Color32::TRANSPARENT
                            })
                            .corner_radius(egui::CornerRadius::same(2))
                            .inner_margin(egui::Margin::symmetric(8, 5))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.add_sized(
                                    [ui.available_width(), 24.0],
                                    egui::Label::new(text).truncate(),
                                );
                            });
                    } else if ui.selectable_label(active, text).clicked() {
                        self.step = *s;
                        self.backups_dirty = true;
                    }
                }
                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);
                ui.weak("EuroScope 3.2.3.2");
            });

        egui::Panel::bottom("footer")
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(30, 34, 40))
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
                            egui::Color32::from_rgb(231, 135, 135)
                        } else {
                            egui::Color32::from_rgb(190, 197, 207)
                        }));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ready = !self.wizard || (self.step_satisfied() && self.job.is_none());
                        if self.step != Step::Done
                            && ui
                                .add_enabled(
                                    ready,
                                    egui::Button::new(if self.wizard {
                                        "Continue"
                                    } else {
                                        "Next"
                                    }),
                                )
                                .clicked()
                        {
                            self.next();
                        }
                        if self.wizard && self.step != Step::Welcome && ui.button("Back").clicked()
                        {
                            let steps = self.steps();
                            if let Some(i) = steps.iter().position(|(s, _)| *s == self.step) {
                                if let Some((previous, _)) =
                                    i.checked_sub(1).and_then(|n| steps.get(n))
                                {
                                    self.step = *previous;
                                }
                            }
                        }
                        if self.wizard
                            && self.step == Step::Done
                            && ui.button("Finish setup").clicked()
                        {
                            self.wizard = false;
                        }
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(25, 28, 33))
                    .inner_margin(egui::Margin::same(24)),
            )
            .show(ui, |ui| {
            if self.wizard {
                let steps = self.steps();
                let current = steps.iter().position(|(s, _)| *s == self.step).unwrap_or(0);
                ui.label(egui::RichText::new(format!("STEP {} OF {}", current + 1, steps.len()))
                    .small().strong().color(egui::Color32::from_rgb(154, 195, 235)));
            }
            ui.heading(match self.step {
                Step::Welcome => "Welcome",
                Step::EuroScope => "EuroScope",
                Step::VcRedist => "Microsoft Visual C++",
                Step::ControllerPack => "UK Controller Pack",
                Step::Updates => "Updates",
                Step::Configure => "Your settings",
                Step::Backups => "Backups",
                Step::Done => "You're all set",
            });
            ui.label(egui::RichText::new(self.step_description())
                .color(egui::Color32::from_rgb(176, 184, 194)));
            ui.add_space(16.0);
            egui::Frame::new()
                .show(ui, |ui| {
            match self.step {
                Step::Welcome => {
                    ui.heading("A simpler way to get set up");
                    ui.label("This installer checks the essentials, installs anything missing and keeps your UK controller pack up to date.");
                    ui.add_space(16.0);
                    ui.label(egui::RichText::new("BEFORE YOU START").small().strong()
                        .color(egui::Color32::from_rgb(150, 160, 173)));
                    ui.add_space(6.0);
                    for (step, name) in [
                        (Step::EuroScope, "EuroScope"),
                        (Step::VcRedist, "Visual C++ runtime"),
                        (Step::ControllerPack, "UK controller pack"),
                    ] {
                        let (status, color) = self.setup_status(step);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("•").color(color).strong());
                            ui.label(name);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(status).small().color(color));
                            });
                        });
                    }
                    ui.add_space(14.0);
                    ui.weak("Your saved settings and screen layout can be kept when the pack is updated.");
                    if let Some(e) = &self.manifest_error {
                        ui.add_space(12.0);
                        ui.colored_label(egui::Color32::from_rgb(230, 180, 112), format!("Could not check for pack updates: {e}"));
                    }
                }
                Step::EuroScope => {
                    self.euroscope_ui(ui);
                }
                Step::VcRedist => {
                    self.vcredist_ui(ui);
                }
                Step::ControllerPack => {
                    self.pack_ui(ui);
                }
                Step::Updates => {
                    self.updates_ui(ui);
                }
                Step::Configure => {
                    self.config_ui(ui);
                }
                Step::Backups => {
                    self.backups_ui(ui);
                }
                Step::Done => {
                    ui.label("EuroScope and the UK controller pack are ready to use.");
                    ui.add_space(8.0);
                    ui.weak("You can return here any time to change your settings, check for updates or restore a backup.");
                }
            }
            });
        });
    }
}
