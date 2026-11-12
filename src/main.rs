//! Replica — grabador/reproductor de rutas con visión de pantalla.
//!
//! F9  = empezar/terminar grabación
//! F10 = reproducir/parar la ruta cargada
//! F12 = parada de emergencia

mod app;
mod bot;
mod model;
mod theme;
mod vision;

fn main() -> eframe::Result<()> {
    let bot = bot::Bot::new();
    bot.log("Replica en línea. F9 grabar · F10 reproducir · F12 cortar.");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Replica")
            .with_app_id("replica")
            .with_inner_size([1040.0, 720.0])
            .with_min_inner_size([760.0, 520.0])
            .with_always_on_top(),
        ..Default::default()
    };
    eframe::run_native("Replica", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, bot)))))
}
