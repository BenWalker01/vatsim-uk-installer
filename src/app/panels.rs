use super::{App, Health, INSTALL_STEPS, STEPS, Step, config_ui};
use crate::install::selfupdate;
use eframe::egui;

const SIDEBAR_WIDTH: f32 = 232.0;

fn chrome_fill(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(30, 34, 40)
    } else {
        egui::Color32::from_rgb(239, 242, 246)
    }
}

fn looks_like_error(status: &str) -> bool {
    ["Failed:", "Could not", "Cannot", "No "]
        .iter()
        .any(|prefix| status.starts_with(prefix))
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.applied_theme != Some(ui.ctx().theme()) {
            self.apply_style(ui.ctx());
        }
        self.poll_job(ui);
        self.poll_self_update_check(ui);
        let busy = self.job.is_some();
        if busy && ui.ctx().input(|i| i.viewport().close_requested()) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.status = "Please wait for the current task to finish before closing.".into();
        }
        self.sidebar(ui, busy);
        self.footer(ui, busy);
        self.central_panel(ui, busy);
    }
}

impl App {
    fn poll_self_update_check(&mut self, ui: &egui::Ui) {
        let Some(handle) = &self.self_update_check else {
            return;
        };
        if !handle.is_finished() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
            return;
        }
        if let Ok(Ok(release)) = self.self_update_check.take().unwrap().join() {
            self.self_update = release;
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, busy: bool) {
        egui::Panel::left("steps")
            .resizable(false)
            .default_size(SIDEBAR_WIDTH)
            .size_range(SIDEBAR_WIDTH..=SIDEBAR_WIDTH)
            .frame(
                egui::Frame::new()
                    .fill(chrome_fill(ui))
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
                self.step_buttons(ui, busy);
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
                    ui.weak(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.horizontal(|ui| {
                        ui.label("Theme");
                        self.theme_ui(ui);
                    });
                });
            });
    }

    fn step_buttons(&mut self, ui: &mut egui::Ui, busy: bool) {
        let dark = ui.visuals().dark_mode;
        for (step, name) in STEPS {
            let active = self.step == *step;
            let health = self.health(*step);
            let (marker, color) = match (*step, health) {
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
                self.step = *step;
                self.backups_dirty = true;
            }
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui, busy: bool) {
        egui::Panel::bottom("footer")
            .frame(
                egui::Frame::new()
                    .fill(chrome_fill(ui))
                    .inner_margin(egui::Margin::symmetric(22, 12)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if !self.status.is_empty() {
                        let color = if looks_like_error(&self.status) {
                            ui.visuals().error_fg_color
                        } else {
                            ui.visuals().text_color()
                        };
                        ui.label(egui::RichText::new(&self.status).color(color));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let can_continue = INSTALL_STEPS.contains(&self.step)
                            && self.health(self.step) == Health::Ok;
                        if can_continue
                            && ui
                                .add_enabled(!busy, egui::Button::new("Continue"))
                                .clicked()
                        {
                            self.next();
                        }
                    });
                });
            });
    }

    fn central_panel(&mut self, ui: &mut egui::Ui, busy: bool) {
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
                self.banners(ui, busy);
                ui.add_space(16.0);
                match self.step {
                    Step::EuroScope => self.euroscope_ui(ui),
                    Step::VcRedist => self.vcredist_ui(ui),
                    Step::ControllerPack => self.pack_ui(ui),
                    Step::Configure => config_ui::show(self, ui),
                    Step::Backups => self.backups_ui(ui),
                }
            });
    }

    fn banners(&mut self, ui: &mut egui::Ui, busy: bool) {
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
                    self.start_self_update_job(move |worker| selfupdate::apply(&release, worker));
                    self.status.clear();
                }
            });
            self.progress_ui(ui, true);
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
    }
}
