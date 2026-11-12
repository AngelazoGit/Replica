//! FarmBot — grabador/reproductor de rutas con visión de pantalla.
//!
//! F9  = empezar/terminar grabación
//! F10 = reproducir/parar la ruta cargada
//! F12 = parada de emergencia

mod app;
mod bot;
mod model;
mod vision;

fn main() -> eframe::Result<()> {
    let bot = bot::Bot::new();
    bot.log("FarmBot listo. F9 grabar · F10 reproducir · F12 parar.");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([980.0, 680.0])
            .with_always_on_top(),
        ..Default::default()
    };
    eframe::run_native("FarmBot", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, bot)))))
}
