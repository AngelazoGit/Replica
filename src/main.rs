//! Replica — grabador/reproductor de rutas con visión de pantalla.
//!
//! F9  = empezar/terminar grabación
//! F10 = reproducir/parar la ruta cargada
//! F11 = pausa / seguir
//! F12 = parada de emergencia
//! F1–F8 = lanzar la ruta que tenga ese atajo

// En Windows, sin ventana de consola detrás de la app.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod bot;
mod config;
mod model;
mod theme;
#[cfg(windows)]
mod tray;
mod vision;

pub const APP_TITLE: &str = "Replica";

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(APP_TITLE)
            .with_app_id("replica")
            .with_inner_size([1320.0, 820.0])
            .with_min_inner_size([1100.0, 640.0])
            .with_icon(theme::icono_app())
            .with_always_on_top(),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        APP_TITLE,
        options,
        Box::new(|cc| {
            // El bot se crea después de la ventana: en Windows, winit ya ha activado el modo
            // DPI-aware y el hook, SendInput y las capturas usan los mismos píxeles físicos.
            let bot = bot::Bot::new();
            let tema = bot.state.lock().unwrap().config.tema;
            theme::configurar_estilo(&cc.egui_ctx, tema);
            bot.log("Replica en línea. F9 grabar · F10 reproducir · F11 pausa · F12 parar.");
            let quit = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            #[allow(unused_mut)]
            let mut app = app::App::new(cc, bot.clone(), quit.clone());
            #[cfg(windows)]
            match tray::crear(&cc.egui_ctx, quit) {
                Ok(t) => app._tray = Some(Box::new(t)),
                Err(e) => bot.log(format!("No se pudo crear el icono de la bandeja: {e}")),
            }
            Ok(Box::new(app))
        }),
    )
}
