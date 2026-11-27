//! Icono en la bandeja del sistema (solo Windows): con la ventana cerrada, los
//! atajos F1–F8 y el programador siguen funcionando. Menú: Mostrar / Salir.

use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// Crea el icono. Hay que conservar el `TrayIcon` devuelto: al soltarlo desaparece.
pub fn crear(ctx: &egui::Context, quit: Arc<AtomicBool>) -> anyhow::Result<TrayIcon> {
    let mostrar = MenuItem::new("Mostrar Replica", true, None);
    let salir = MenuItem::new("Salir", true, None);
    let menu = Menu::new();
    menu.append_items(&[&mostrar, &PredefinedMenuItem::separator(), &salir])?;
    let (id_mostrar, id_salir) = (mostrar.id().clone(), salir.id().clone());

    let ic = crate::theme::icono_app();
    let icon = tray_icon::Icon::from_rgba(ic.rgba, ic.width, ic.height)?;
    let tray = TrayIconBuilder::new()
        .with_tooltip("Replica")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .build()?;

    // Con la ventana oculta egui no se repinta, así que los eventos se atienden aquí
    // (desde el hilo del sistema) y se le pide a la ventana que se muestre o se cierre.
    let c = ctx.clone();
    MenuEvent::set_event_handler(Some(Arc::new(move |e: MenuEvent| {
        if e.id == id_mostrar {
            mostrar_ventana(&c);
        } else if e.id == id_salir {
            quit.store(true, Ordering::SeqCst);
            c.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            c.send_viewport_cmd(egui::ViewportCommand::Close);
            c.request_repaint();
        }
    })));
    let c = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = e
        {
            mostrar_ventana(&c);
        }
    }));
    Ok(tray)
}

fn mostrar_ventana(c: &egui::Context) {
    c.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    c.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    c.send_viewport_cmd(egui::ViewportCommand::Focus);
    c.request_repaint();
}
