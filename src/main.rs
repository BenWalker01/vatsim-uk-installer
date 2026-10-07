#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod backup;
mod config;
mod download;
mod euroscope;
mod layout;
mod manifest;
mod pack;
mod selfupdate;
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
    selfupdate::cleanup();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([860.0, 760.0])
            .with_min_inner_size([860.0, 760.0])
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "VATSIM UK Installer",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc.egui_ctx.clone())))),
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
