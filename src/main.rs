mod app;
mod backup;
mod config;
mod download;
mod euroscope;
mod layout;
mod manifest;
mod pack;
mod state;
mod updater;
mod vcredist;

fn app_icon() -> eframe::egui::IconData {
    let icon = image::load_from_memory_with_format(
        include_bytes!("../data/logo.ico"),
        image::ImageFormat::Ico,
    )
    .expect("embedded application icon must be a valid ICO file")
    .into_rgba8();
    let (width, height) = (icon.width(), icon.height());
    eframe::egui::IconData {
        rgba: icon.into_raw(),
        width,
        height,
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([860.0, 760.0])
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "VATSIM UK Installer",
        options,
        Box::new(|cc| {
            use eframe::egui::{Color32, FontId, Stroke, TextStyle, Visuals};
            cc.egui_ctx.all_styles_mut(|style| {
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
                let mut visuals = Visuals::dark();
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
                visuals.widgets.noninteractive.corner_radius = eframe::egui::CornerRadius::same(3);
                visuals.widgets.inactive.corner_radius = eframe::egui::CornerRadius::same(3);
                visuals.widgets.hovered.corner_radius = eframe::egui::CornerRadius::same(3);
                visuals.widgets.active.corner_radius = eframe::egui::CornerRadius::same(3);
                style.visuals = visuals;
            });
            Ok(Box::new(app::App::new()))
        }),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_icon_decodes_to_rgba() {
        let icon = super::app_icon();
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
        assert!(icon.width > 0 && icon.height > 0);
    }
}
