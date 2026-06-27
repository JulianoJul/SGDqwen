mod app;
mod db;
mod models;
mod storage;

fn main() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([700.0, 450.0]),
        ..Default::default()
    };

    eframe::run_native(
        "SGD - Sistema de Gestión Documental",
        options,
        Box::new(|_cc| {
            let app = app::SgdApp::new().expect("Failed to initialize SGD application");
            Box::new(app)
        }),
    )
    .expect("Failed to run SGD application");
}
