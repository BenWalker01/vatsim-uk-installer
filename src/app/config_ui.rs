use super::{App, ConfigTab};
use crate::{
    config::{self, Config},
    layout, pack,
};
use eframe::egui;

pub(super) fn show(app: &mut App, ui: &mut egui::Ui) {
    app.config_ui(ui);
}

impl App {
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
        let scroll_height = (ui.available_height() - 100.0).max(120.0);
        let saved_layout_count = layout::load_layout().len();
        egui::ScrollArea::vertical()
            .max_height(scroll_height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(6.0);
                match self.config_tab {
                    ConfigTab::Details => Self::details_tab(ui, &mut self.config),
                    ConfigTab::Appearance => {
                        Self::appearance_tab(ui, &mut self.config, &mut self.textures)
                    }
                    ConfigTab::Controlling => {
                        Self::controlling_tab(ui, &mut self.config, &mut self.capturing_asel)
                    }
                    ConfigTab::Layout => {
                        self.layout_tab(ui, &dir, saved_layout_count);
                    }
                }
            });
        ui.add_space(6.0);
        self.progress_ui(ui);
        let idle = self.job.is_none();
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let button =
                egui::Button::new(egui::RichText::new("Save and apply").size(18.0).strong())
                    .fill(ui.visuals().selection.bg_fill)
                    .min_size(egui::vec2(200.0, 40.0));
            if ui.add_enabled(idle && problem.is_none(), button).clicked() {
                let cfg = self.config.clone();
                self.start_job(move |worker| {
                    config::save(&cfg)?;
                    config::apply(&cfg, &dir, worker)
                });
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

    fn details_tab(ui: &mut egui::Ui, config: &mut Config) {
        let hint = |ui: &mut egui::Ui, invalid: bool, message: &str| {
            if invalid {
                ui.colored_label(ui.visuals().error_fg_color, message);
            }
        };

        egui::Grid::new("basic")
            .num_columns(3)
            .spacing([16.0, 12.0])
            .show(ui, |ui| {
                ui.label("Name (as on VATSIM)");
                ui.add_sized([360.0, 30.0], egui::TextEdit::singleline(&mut config.name));
                hint(ui, config.name.trim().is_empty(), "Required");
                ui.end_row();

                ui.label("Initials");
                ui.add_sized(
                    [360.0, 30.0],
                    egui::TextEdit::singleline(&mut config.initials).hint_text("2-3 letters"),
                );
                hint(
                    ui,
                    !(2..=3).contains(&config.initials.trim().chars().count()),
                    "2-3 letters",
                );
                ui.end_row();

                ui.label("VATSIM CID");
                ui.add_sized([360.0, 30.0], egui::TextEdit::singleline(&mut config.cid));
                hint(ui, !config::is_valid_cid(&config.cid), "6 or 7 digits");
                ui.end_row();

                ui.label("Rating");
                let rating_index = config
                    .rating
                    .parse::<usize>()
                    .unwrap_or(0)
                    .min(config::RATINGS.len() - 1);
                egui::ComboBox::from_id_salt("rating")
                    .selected_text(config::RATINGS[rating_index])
                    .show_ui(ui, |ui| {
                        for (index, rating) in config::RATINGS.iter().enumerate() {
                            ui.selectable_value(&mut config.rating, index.to_string(), *rating);
                        }
                    });
                ui.end_row();

                ui.label("VATSIM password");
                ui.add_sized(
                    [360.0, 30.0],
                    egui::TextEdit::singleline(&mut config.password).password(true),
                );
                hint(ui, config.password.is_empty(), "Required");
                ui.end_row();

                ui.label("Hoppie CPDLC code");
                ui.add_sized(
                    [360.0, 30.0],
                    egui::TextEdit::singleline(&mut config.cpdlc).hint_text("optional"),
                );
                ui.end_row();
            });
    }

    fn appearance_tab(
        ui: &mut egui::Ui,
        config: &mut Config,
        textures: &mut std::collections::HashMap<String, egui::TextureHandle>,
    ) {
        let font_size = config::FONT_OPTIONS
            .iter()
            .find(|option| option.0 == config.font_size)
            .map_or("", |option| option.1);

        egui::Grid::new("appearance")
            .num_columns(2)
            .spacing([16.0, 12.0])
            .show(ui, |ui| {
                ui.label("Text size");
                egui::ComboBox::from_id_salt("font_size")
                    .selected_text(font_size)
                    .show_ui(ui, |ui| {
                        for (key, label, _) in config::FONT_OPTIONS {
                            ui.selectable_value(&mut config.font_size, key.to_string(), *label);
                        }
                    });
                ui.end_row();

                Self::choice_ui(
                    ui,
                    textures,
                    "coastline",
                    "Coastline colour",
                    config::COAST_OPTIONS,
                    &mut config.coast_choice,
                );
                Self::choice_ui(
                    ui,
                    textures,
                    "land",
                    "Land colour",
                    config::LAND_OPTIONS,
                    &mut config.land_choice,
                );
            });
        ui.add_space(8.0);
        ui.weak(
            "Text size applies to metar, chat and list headers. Open a colour list and hover an entry to preview it.",
        );
    }

    fn controlling_tab(ui: &mut egui::Ui, config: &mut Config, capturing_asel: &mut bool) {
        Self::yes_no(
            ui,
            "Realistic datablocks for LAC/LTC (no climb/descent arrows)",
            &mut config.realistic_tags,
        );
        Self::yes_no(
            ui,
            "Realistic code/callsign conversion",
            &mut config.realistic_conversion,
        );
        Self::yes_no(
            ui,
            "DiscordEuroscope plugin (shows where you're controlling)",
            &mut config.discord_presence,
        );

        let rdf_mode = config::RDF_OPTIONS
            .iter()
            .find(|option| option.0 == config.rdf_mode)
            .map_or("", |option| option.1);
        ui.horizontal(|ui| {
            ui.label("RDF (radio direction finding)");
            egui::ComboBox::from_id_salt("rdf")
                .selected_text(rdf_mode)
                .show_ui(ui, |ui| {
                    for (key, label) in config::RDF_OPTIONS {
                        ui.selectable_value(&mut config.rdf_mode, key.to_string(), *label);
                    }
                });
        });

        ui.horizontal(|ui| {
            let bound = if config.asel_key.is_empty() {
                "default (NUMPLUS)".to_string()
            } else {
                config::asel_name(&config.asel_key)
                    .unwrap_or_else(|| format!("custom (code {})", config.asel_key))
            };
            ui.label(format!("ASEL key: {bound}"));
            if *capturing_asel {
                ui.label("Press a key (Esc to cancel)...");
            } else if ui.button("Set").clicked() {
                *capturing_asel = true;
            }
            if !config.asel_key.is_empty() && ui.button("Reset").clicked() {
                config.asel_key.clear();
            }
        });
    }

    fn layout_tab(&mut self, ui: &mut egui::Ui, pack_dir: &std::path::Path, saved_count: usize) {
        ui.weak(
            "Move your windows in EuroScope and save your ASRs, then save your layout. It is re-applied whenever you apply configuration.",
        );
        ui.horizontal(|ui| {
            let idle = self.job.is_none();
            if ui
                .add_enabled(idle, egui::Button::new("Save current layout"))
                .clicked()
            {
                let release = self.state.pack_version.and_then(|version| {
                    self.manifest
                        .as_ref()?
                        .releases
                        .iter()
                        .find(|release| release.version == version)
                        .cloned()
                });
                match release {
                    Some(release) => {
                        let pack_dir = pack_dir.to_path_buf();
                        self.start_job(move |worker| {
                            let pristine = pack::fetch_pristine_asrs(&release, worker)?;
                            layout::save_changes(&pack_dir, &pristine).map(|_| ())
                        });
                        self.status.clear();
                    }
                    None => {
                        self.status =
                            "Installed pack version not found in the release list (offline?)."
                                .into();
                    }
                }
            }
            if ui
                .add_enabled(
                    saved_count > 0 && idle,
                    egui::Button::new("Clear saved layout"),
                )
                .clicked()
            {
                self.status = match layout::clear_layout() {
                    Ok(()) => "Saved layout cleared.".into(),
                    Err(error) => format!("Could not clear layout: {error}"),
                };
            }
        });

        let summary = if saved_count > 0 {
            format!("{saved_count} ASR file(s) with saved changes")
        } else {
            "No layout saved yet.".into()
        };
        ui.label(summary);
    }
}
