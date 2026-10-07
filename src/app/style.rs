use super::App;
use super::Health;
use crate::state::ThemePreference;
use eframe::egui;

impl App {
    pub(super) fn status_color(health: Health, dark_mode: bool) -> egui::Color32 {
        match (health, dark_mode) {
            (Health::Ok, true) => egui::Color32::from_rgb(110, 190, 145),
            (Health::Ok, false) => egui::Color32::from_rgb(32, 126, 78),
            (Health::Outdated, true) => egui::Color32::from_rgb(240, 200, 70),
            (Health::Outdated, false) => egui::Color32::from_rgb(176, 124, 0),
            (Health::Missing, true) => egui::Color32::from_rgb(158, 165, 174),
            (Health::Missing, false) => egui::Color32::from_rgb(99, 108, 119),
        }
    }

    pub(super) fn apply_style(&mut self, ctx: &egui::Context) {
        let theme = ctx.theme();
        ctx.all_styles_mut(|style| {
            use egui::{Color32, FontId, Stroke, TextStyle};
            for (text_style, size) in [
                (TextStyle::Body, 16.0),
                (TextStyle::Button, 16.0),
                (TextStyle::Small, 13.0),
                (TextStyle::Monospace, 15.0),
                (TextStyle::Heading, 26.0),
            ] {
                style
                    .text_styles
                    .insert(text_style, FontId::proportional(size));
            }
            style.spacing.item_spacing = [12.0, 10.0].into();
            style.spacing.button_padding = [14.0, 8.0].into();
            style.spacing.interact_size.y = 34.0;
            let mut visuals = theme.default_visuals();
            if theme == egui::Theme::Dark {
                visuals.panel_fill = Color32::from_rgb(25, 28, 33);
                visuals.window_fill = Color32::from_rgb(32, 36, 42);
                visuals.extreme_bg_color = Color32::from_rgb(19, 22, 26);
                visuals.faint_bg_color = Color32::from_rgb(39, 44, 51);
                visuals.selection.bg_fill = Color32::from_rgb(47, 71, 96);
                visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(117, 169, 222));
                visuals.widgets.inactive.bg_fill = Color32::from_rgb(39, 44, 51);
                visuals.widgets.inactive.bg_stroke =
                    Stroke::new(1.0, Color32::from_rgb(57, 64, 73));
                visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 56, 66);
                visuals.widgets.hovered.bg_stroke =
                    Stroke::new(1.0, Color32::from_rgb(91, 117, 145));
                visuals.widgets.active.bg_fill = Color32::from_rgb(47, 71, 96);
                visuals.widgets.active.bg_stroke =
                    Stroke::new(1.0, Color32::from_rgb(117, 169, 222));
            } else {
                visuals.panel_fill = Color32::from_rgb(250, 251, 253);
                visuals.window_fill = Color32::WHITE;
                visuals.extreme_bg_color = Color32::WHITE;
                visuals.faint_bg_color = Color32::from_rgb(239, 242, 246);
                visuals.selection.bg_fill = Color32::from_rgb(218, 232, 248);
                visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(54, 105, 160));
                visuals.widgets.inactive.bg_fill = Color32::from_rgb(244, 246, 249);
                visuals.widgets.inactive.bg_stroke =
                    Stroke::new(1.0, Color32::from_rgb(210, 216, 224));
                visuals.widgets.hovered.bg_fill = Color32::from_rgb(232, 239, 247);
                visuals.widgets.hovered.bg_stroke =
                    Stroke::new(1.0, Color32::from_rgb(133, 160, 190));
                visuals.widgets.active.bg_fill = Color32::from_rgb(210, 228, 247);
                visuals.widgets.active.bg_stroke =
                    Stroke::new(1.0, Color32::from_rgb(54, 105, 160));
            }
            for widget in [
                &mut visuals.widgets.noninteractive,
                &mut visuals.widgets.inactive,
                &mut visuals.widgets.hovered,
                &mut visuals.widgets.active,
            ] {
                widget.corner_radius = egui::CornerRadius::same(3);
            }
            style.visuals = visuals;
        });
        self.applied_theme = Some(theme);
    }

    pub(super) fn theme_ui(&mut self, ui: &mut egui::Ui) {
        let mut preference = self.state.theme;
        egui::ComboBox::from_id_salt("theme_preference")
            .selected_text(preference.label())
            .show_ui(ui, |ui| {
                for option in [
                    ThemePreference::Auto,
                    ThemePreference::Light,
                    ThemePreference::Dark,
                ] {
                    ui.selectable_value(&mut preference, option, option.label());
                }
            });
        if preference != self.state.theme {
            self.state.theme = preference;
            ui.ctx().set_theme(preference.egui());
            if let Err(e) = self.state.save() {
                self.status = format!("Could not save theme preference: {e}");
            }
        }
    }
}
