use super::App;
use eframe::egui;

impl App {
    /// Re-check button with a brief spinner, then a tick/cross result. Returns true when the check should run.
    pub(super) fn recheck_ui(&mut self, ui: &mut egui::Ui, key: &'static str, ok: bool) -> bool {
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
}
