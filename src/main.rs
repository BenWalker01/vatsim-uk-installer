mod app;
mod download;
mod euroscope;
mod manifest;
mod pack;
mod state;
mod updater;
mod vcredist;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([720.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "VATSIM UK Installer",
        options,
        Box::new(|_cc| Ok(Box::new(app::App::new()))),
    )
}
