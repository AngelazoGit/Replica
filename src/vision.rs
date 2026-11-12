//! Visión: captura de pantalla y búsqueda de plantillas (template matching).
//!
//! Al grabar, en cada clic se guarda un recorte de la pantalla alrededor del cursor
//! (el "ancla"). Al reproducir se busca ese recorte cerca de la posición original:
//! - si aparece → se clica donde está ahora (se adapta si la ventana/UI se movió);
//! - si no aparece → se espera (p.ej. a que cargue una pantalla) hasta un timeout.
//!
//! El ancla se toma de un fotograma capturado *antes* del clic (ver `Bot`), para
//! no guardar el estado de la pantalla después de que el juego reaccione al clic.

use crate::model::Target;
use image::{GrayImage, RgbaImage};
use xcap::Monitor;

/// Lado del recorte que se guarda como ancla (píxeles).
pub const ANCHOR_SIZE: u32 = 48;

/// Captura la región `(x, y, w, h)` en coordenadas globales de pantalla.
/// La región se recorta a los límites del monitor; devuelve la imagen y su
/// origen real en coordenadas globales. La imagen siempre está en las mismas
/// unidades que el ratón (en Retina/HiDPI la captura física se reescala).
pub fn capture_region(x: i32, y: i32, w: u32, h: u32) -> anyhow::Result<(RgbaImage, i32, i32)> {
    let mon = Monitor::from_point(x + w as i32 / 2, y + h as i32 / 2)
        .or_else(|_| Monitor::from_point(x.max(0), y.max(0)))?;
    let full = mon.capture_image()?;
    let (mw, mh) = (mon.width().max(1) as i32, mon.height().max(1) as i32);
    let lx = (x - mon.x()).clamp(0, mw - 1);
    let ly = (y - mon.y()).clamp(0, mh - 1);
    let w = (w as i32).min(mw - lx) as u32;
    let h = (h as i32).min(mh - ly) as u32;
    // Píxeles de imagen por unidad de ratón (2.0 en Retina, 1.0 normalmente).
    let s = full.width() as f64 / mw as f64;
    let img = if (s - 1.0).abs() < 0.01 {
        image::imageops::crop_imm(&full, lx as u32, ly as u32, w, h).to_image()
    } else {
        let px = |v: f64| (v * s).round() as u32;
        let (cx, cy) = (
            px(lx as f64).min(full.width() - 1),
            px(ly as f64).min(full.height() - 1),
        );
        let cw = px(w as f64).clamp(1, full.width() - cx);
        let ch = px(h as f64).clamp(1, full.height() - cy);
        let crop = image::imageops::crop_imm(&full, cx, cy, cw, ch).to_image();
        image::imageops::resize(
            &crop,
            w.max(1),
            h.max(1),
            image::imageops::FilterType::Triangle,
        )
    };
    Ok((img, mon.x() + lx, mon.y() + ly))
}

/// Ventanas visibles de otras aplicaciones (excluye la propia Replica).
pub fn list_windows() -> Vec<Target> {
    xcap::Window::all()
        .unwrap_or_default()
        .into_iter()
        // xcap sólo da el PID en Windows: la ventana propia se reconoce por su título.
        .filter(|w| {
            w.title() != crate::APP_TITLE && !w.is_minimized() && w.width() > 1 && w.height() > 1
        })
        .map(|w| Target {
            app: w.app_name().to_string(),
            title: w.title().to_string(),
            x: w.x(),
            y: w.y(),
            w: w.width(),
            h: w.height(),
        })
        .collect()
}

/// Ventana bajo un punto de la pantalla.
pub fn window_at(x: f64, y: f64) -> Option<Target> {
    let (x, y) = (x as i32, y as i32);
    let mut hits = list_windows()
        .into_iter()
        .filter(|t| x >= t.x && y >= t.y && x < t.x + t.w as i32 && y < t.y + t.h as i32);
    if cfg!(target_os = "linux") {
        // X11 no da el orden Z: la ventana más pequeña que contiene el punto es la de delante.
        hits.min_by_key(|t| t.w as u64 * t.h as u64)
    } else {
        hits.next() // Windows y macOS listan de delante hacia atrás
    }
}

/// Busca ahora la ventana grabada: misma aplicación, preferiblemente mismo título.
pub fn find_window(t: &Target) -> Option<Target> {
    let same_app: Vec<Target> = list_windows()
        .into_iter()
        .filter(|w| w.app == t.app)
        .collect();
    same_app
        .iter()
        .find(|w| w.title == t.title)
        .or(same_app.first())
        .cloned()
}

/// Recorta el ancla de un fotograma ya capturado `(img, x0, y0)` si la contiene entera.
pub fn anchor_from_frame(frame: &(RgbaImage, i32, i32), cx: f64, cy: f64) -> Option<RgbaImage> {
    let (img, x0, y0) = frame;
    let half = (ANCHOR_SIZE / 2) as i32;
    let (lx, ly) = (cx as i32 - half - x0, cy as i32 - half - y0);
    let fits = lx >= 0
        && ly >= 0
        && lx as u32 + ANCHOR_SIZE <= img.width()
        && ly as u32 + ANCHOR_SIZE <= img.height();
    fits.then(|| {
        image::imageops::crop_imm(img, lx as u32, ly as u32, ANCHOR_SIZE, ANCHOR_SIZE).to_image()
    })
}

/// Recorte centrado en un punto (para crear anclas al grabar).
pub fn capture_anchor(cx: f64, cy: f64) -> anyhow::Result<RgbaImage> {
    let half = (ANCHOR_SIZE / 2) as i32;
    Ok(capture_region(cx as i32 - half, cy as i32 - half, ANCHOR_SIZE, ANCHOR_SIZE)?.0)
}

pub struct Match {
    /// Centro de la plantilla encontrada (coordenadas globales).
    pub x: f64,
    pub y: f64,
    /// Similitud 0.0..=1.0 (1.0 = idéntico).
    pub score: f32,
}

/// Busca la plantilla que mejor encaja (p.ej. escalada y original) en un área de
/// ±`radius` px alrededor de `(cx, cy)`. Una sola captura para todas.
pub fn find_near(templates: &[GrayImage], cx: f64, cy: f64, radius: u32) -> anyhow::Result<Match> {
    let tw = templates.iter().map(|t| t.width()).max().unwrap_or(0);
    let th = templates.iter().map(|t| t.height()).max().unwrap_or(0);
    let x0 = cx as i32 - (tw / 2) as i32 - radius as i32;
    let y0 = cy as i32 - (th / 2) as i32 - radius as i32;
    let (area, ox, oy) = capture_region(x0, y0, tw + 2 * radius, th + 2 * radius)?;
    let area = image::imageops::grayscale(&area);
    templates
        .iter()
        .map(|t| {
            let (tw, th) = t.dimensions();
            // Posición donde estaba la plantilla al grabar (dentro del área capturada).
            let ex = (cx as i32 - (tw / 2) as i32 - ox).max(0) as u32;
            let ey = (cy as i32 - (th / 2) as i32 - oy).max(0) as u32;
            let (bx, by, score) = best_match(&area, t, (ex, ey));
            Match {
                x: (ox + bx as i32) as f64 + tw as f64 / 2.0,
                y: (oy + by as i32) as f64 + th as f64 / 2.0,
                score,
            }
        })
        .max_by(|a, b| a.score.total_cmp(&b.score))
        .ok_or_else(|| anyhow::anyhow!("sin plantillas"))
}

/// Búsqueda por suma de diferencias absolutas (SAD) sobre escala de grises.
/// `prefer` es la posición esperada: sólo se abandona si otra es claramente
/// mejor (evita saltos en zonas de color uniforme). Devuelve (x, y, similitud).
pub fn best_match(hay: &GrayImage, needle: &GrayImage, prefer: (u32, u32)) -> (u32, u32, f32) {
    let (hw, hh) = hay.dimensions();
    let (nw, nh) = needle.dimensions();
    if nw > hw || nh > hh || nw == 0 || nh == 0 {
        return (0, 0, 0.0);
    }
    let max_x = hw - nw;
    let max_y = hh - nh;
    let (hr, nr) = (hay.as_raw(), needle.as_raw());
    let (hw, nw_us) = (hw as usize, nw as usize);
    let sad = |x: u32, y: u32, limit: u64| -> u64 {
        let mut s = 0u64;
        for j in 0..nh as usize {
            let h0 = (y as usize + j) * hw + x as usize;
            let row = hr[h0..h0 + nw_us]
                .iter()
                .zip(&nr[j * nw_us..(j + 1) * nw_us]);
            s += row.map(|(a, b)| a.abs_diff(*b) as u64).sum::<u64>();
            if s >= limit {
                return s; // poda: ya es peor que el mejor
            }
        }
        s
    };

    // Búsqueda exhaustiva con poda por filas: rápido en la práctica porque
    // las posiciones malas superan enseguida el mejor SAD actual.
    let margin = (nw * nh) as u64 * 255 / 100; // 1 % de diferencia
    let (px, py) = (prefer.0.min(max_x), prefer.1.min(max_y));
    let mut best = (px, py, sad(px, py, u64::MAX).saturating_sub(margin));
    for y in 0..=max_y {
        for x in 0..=max_x {
            let s = sad(x, y, best.2);
            if s < best.2 {
                best = (x, y, s);
            }
        }
    }
    let max_sad = (nw * nh) as f32 * 255.0;
    let real = sad(best.0, best.1, u64::MAX);
    (best.0, best.1, 1.0 - real as f32 / max_sad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_template_in_image() {
        let hay = GrayImage::from_fn(200, 150, |x, y| {
            let h = (x.wrapping_mul(73856093) ^ y.wrapping_mul(19349663)).wrapping_mul(2654435761);
            image::Luma([(h >> 24) as u8])
        });
        let needle = image::imageops::crop_imm(&hay, 77, 41, 30, 30).to_image();
        let (x, y, score) = best_match(&hay, &needle, (0, 0));
        assert_eq!((x, y), (77, 41));
        assert!(score > 0.999);
    }

    #[test]
    fn anchor_from_frame_needs_full_square() {
        let frame = (RgbaImage::new(256, 256), 1000, 500);
        assert!(anchor_from_frame(&frame, 1128.0, 628.0).is_some());
        assert!(anchor_from_frame(&frame, 1010.0, 628.0).is_none()); // se sale por la izquierda
    }

    #[test]
    fn uniform_area_keeps_expected_position() {
        let hay = GrayImage::from_pixel(120, 120, image::Luma([10]));
        let needle = GrayImage::from_pixel(20, 20, image::Luma([10]));
        assert_eq!(best_match(&hay, &needle, (50, 60)).0, 50);
        assert_eq!(best_match(&hay, &needle, (50, 60)).1, 60);
    }
}
