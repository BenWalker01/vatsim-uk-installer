use super::App;
use crate::euroscope;
use eframe::egui;

impl App {
    pub(super) fn euroscope_ui(&mut self, ui: &mut egui::Ui) {
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
                        self.start_job(move |worker| euroscope::install(&url, worker));
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
}
