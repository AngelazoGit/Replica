//! Replica — grabador/reproductor de rutas con visión de pantalla.
//!
//! F9  = empezar/terminar grabación
//! F10 = reproducir/parar la ruta cargada
//! F12 = parada de emergencia

// En Windows, sin ventana de consola detrás de la app.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod bot;
mod model;
mod theme;
mod vision;

pub const APP_TITLE: &str = "Replica";

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(APP_TITLE)
            .with_app_id("replica")
            .with_inner_size([1040.0, 720.0])
            .with_min_inner_size([760.0, 520.0])
            .with_always_on_top(),
        ..Default::default()
    };
    eframe::run_native(
        APP_TITLE,
        options,
        Box::new(|cc| {
            // El bot se crea después de la ventana: en Windows, winit ya ha activado el modo
            // DPI-aware y el hook, SendInput y las capturas usan los mismos píxeles físicos.
            let bot = bot::Bot::new();
            bot.log("Replica en línea. F9 grabar · F10 reproducir · F12 cortar.");
            Ok(Box::new(app::App::new(cc, bot)))
        }),
    )
}
