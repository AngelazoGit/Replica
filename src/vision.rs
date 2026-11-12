//! Visión: captura de pantalla y búsqueda de plantillas (template matching).
//!
//! Al grabar, en cada clic se guarda un recorte de la pantalla alrededor del cursor
//! (el "ancla"). Al reproducir se busca ese recorte cerca de la posición original:
//! - si aparece → se clica donde está ahora (se adapta si la ventana/UI se movió);
//! - si no aparece → se espera (p.ej. a que cargue una pantalla) hasta un timeout.

use image::{GrayImage, RgbaImage};
use xcap::Monitor;

/// Lado del recorte que se guarda como ancla (píxeles).
pub const ANCHOR_SIZE: u32 = 48;

/// Captura la región `(x, y, w, h)` en coordenadas globales de pantalla.
/// La región se recorta a los límites del monitor; devuelve la imagen y su
/// origen real en coordenadas globales.
pub fn capture_region(x: i32, y: i32, w: u32, h: u32) -> anyhow::Result<(RgbaImage, i32, i32)> {
    let mon = Monitor::from_point(x + w as i32 / 2, y + h as i32 / 2)
        .or_else(|_| Monitor::from_point(x.max(0), y.max(0)))?;
    let full = mon.capture_image()?;
    // Coordenadas locales al monitor, recortadas a sus límites.
    let lx = (x - mon.x()).clamp(0, full.width() as i32 - 1) as u32;
    let ly = (y - mon.y()).clamp(0, full.height() as i32 - 1) as u32;
    let w = w.min(full.width() - lx);
    let h = h.min(full.height() - ly);
    let img = image::imageops::crop_imm(&full, lx, ly, w, h).to_image();
    Ok((img, mon.x() + lx as i32, mon.y() + ly as i32))
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

/// Busca `template` en un área de ±`radius` px alrededor de `(cx, cy)`.
pub fn find_near(template: &RgbaImage, cx: f64, cy: f64, radius: u32) -> anyhow::Result<Match> {
    let (tw, th) = template.dimensions();
    let ox = cx as i32 - (tw / 2) as i32 - radius as i32;
    let oy = cy as i32 - (th / 2) as i32 - radius as i32;
    let (area, ox, oy) = capture_region(ox, oy, tw + 2 * radius, th + 2 * radius)?;
    // Posición donde estaba la plantilla al grabar (dentro del área capturada).
    let ex = (cx as i32 - (tw / 2) as i32 - ox).max(0) as u32;
    let ey = (cy as i32 - (th / 2) as i32 - oy).max(0) as u32;
    let (bx, by, score) = best_match(
        &image::imageops::grayscale(&area),
        &image::imageops::grayscale(template),
        (ex, ey),
    );
    Ok(Match {
        x: (ox + bx as i32) as f64 + tw as f64 / 2.0,
        y: (oy + by as i32) as f64 + th as f64 / 2.0,
        score,
    })
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
            let row = hr[h0..h0 + nw_us].iter().zip(&nr[j * nw_us..(j + 1) * nw_us]);
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
        let hay = GrayImage::from_fn(200, 150, |x, y| { let h = (x.wrapping_mul(73856093) ^ y.wrapping_mul(19349663)).wrapping_mul(2654435761); image::Luma([(h >> 24) as u8]) });
        let needle = image::imageops::crop_imm(&hay, 77, 41, 30, 30).to_image();
        let (x, y, score) = best_match(&hay, &needle, (0, 0));
        assert_eq!((x, y), (77, 41));
        assert!(score > 0.999);
    }

    #[test]
    fn uniform_area_keeps_expected_position() {
        let hay = GrayImage::from_pixel(120, 120, image::Luma([10]));
        let needle = GrayImage::from_pixel(20, 20, image::Luma([10]));
        assert_eq!(best_match(&hay, &needle, (50, 60)).0, 50);
        assert_eq!(best_match(&hay, &needle, (50, 60)).1, 60);
    }
}
