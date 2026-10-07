use super::App;
use crate::install::vcredist;
use eframe::egui;

impl App {
    pub(super) fn vcredist_ui(&mut self, ui: &mut egui::Ui) {
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
                        self.start_job(move |worker| vcredist::install(&url, worker));
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
}
