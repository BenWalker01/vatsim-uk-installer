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
            .with_inner_size([860.0, 620.0])
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "VATSIM UK Installer",
        options,
        Box::new(|cc| {
            use eframe::egui::{FontId, TextStyle};
            cc.egui_ctx.all_styles_mut(|style| {
                for (text_style, size) in [
                    (TextStyle::Body, 16.0),
                    (TextStyle::Button, 16.0),
                    (TextStyle::Small, 13.0),
                    (TextStyle::Monospace, 15.0),
                    (TextStyle::Heading, 26.0),
                ] {
                    style.text_styles.insert(text_style, FontId::proportional(size));
                }
                style.spacing.item_spacing = [10.0, 8.0].into();
                style.spacing.button_padding = [10.0, 5.0].into();
                style.spacing.interact_size.y = 26.0;
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
