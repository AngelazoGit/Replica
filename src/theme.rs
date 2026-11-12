//! Estética de Replica: paleta "Sodio y Ultravioleta".
//! Ámbar de farola de sodio + violeta UV + verde fósforo sobre negro tintado.

use eframe::egui::{
    self, Color32, FontFamily, FontId, Rect, Rounding, Stroke, TextStyle, Ui, Vec2,
};

pub const VOID: Color32 = Color32::from_rgb(0x0A, 0x09, 0x10);
pub const PANEL: Color32 = Color32::from_rgb(0x11, 0x0F, 0x1A);
pub const PANEL_HI: Color32 = Color32::from_rgb(0x1B, 0x18, 0x28);
pub const LINE: Color32 = Color32::from_rgb(0x2E, 0x28, 0x44);
pub const SODIUM: Color32 = Color32::from_rgb(0xFF, 0x9E, 0x3D);
pub const UV: Color32 = Color32::from_rgb(0x9D, 0x7B, 0xFF);
pub const PHOSPHOR: Color32 = Color32::from_rgb(0x4D, 0xFF, 0xB8);
pub const CORAL: Color32 = Color32::from_rgb(0xFF, 0x4F, 0x6E);
pub const BONE: Color32 = Color32::from_rgb(0xE9, 0xE3, 0xD3);
pub const DIM: Color32 = Color32::from_rgb(0x7A, 0x72, 0x90);

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Heading, FontId::new(17.0, FontFamily::Monospace)),
        (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(13.5, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(12.5, FontFamily::Monospace),
        ),
        (TextStyle::Small, FontId::new(10.5, FontFamily::Monospace)),
    ]
    .into();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 4.0);
    style.spacing.slider_width = 120.0;

    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.override_text_color = None;
    v.panel_fill = PANEL;
    v.window_fill = PANEL_HI;
    v.window_stroke = Stroke::new(1.0_f32, UV);
    v.window_rounding = Rounding::ZERO;
    v.menu_rounding = Rounding::ZERO;
    v.extreme_bg_color = VOID; // fondo de campos de texto
    v.faint_bg_color = PANEL_HI;
    v.code_bg_color = VOID;
    v.hyperlink_color = UV;
    v.warn_fg_color = SODIUM;
    v.error_fg_color = CORAL;
    v.selection.bg_fill = UV.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0_f32, UV);
    v.slider_trailing_fill = true;
    v.text_cursor.stroke = Stroke::new(2.0_f32, SODIUM);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, BONE);
    w.inactive.bg_fill = PANEL_HI;
    w.inactive.weak_bg_fill = PANEL_HI;
    w.inactive.bg_stroke = Stroke::new(1.0_f32, LINE);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, BONE);
    w.hovered.bg_fill = LINE;
    w.hovered.weak_bg_fill = Color32::from_rgb(0x26, 0x20, 0x38);
    w.hovered.bg_stroke = Stroke::new(1.0_f32, SODIUM);
    w.hovered.fg_stroke = Stroke::new(1.5_f32, SODIUM);
    w.active.bg_fill = SODIUM.gamma_multiply(0.3);
    w.active.weak_bg_fill = SODIUM.gamma_multiply(0.3);
    w.active.bg_stroke = Stroke::new(1.5_f32, SODIUM);
    w.active.fg_stroke = Stroke::new(2.0_f32, SODIUM);
    w.open = w.active;
    for wv in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        wv.rounding = Rounding::ZERO;
        wv.expansion = 0.0;
    }
    ctx.set_style(style);
}

/// Cabecera de sección: barra vertical de color + título en mayúsculas monoespaciado.
pub fn section(ui: &mut Ui, title: &str, color: Color32) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(3.0, 14.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, Rounding::ZERO, color);
        ui.label(egui::RichText::new(title).monospace().color(color).strong());
        let rest = ui.available_rect_before_wrap();
        let y = rest.center().y;
        ui.painter()
            .hline(rest.left()..=rest.right(), y, Stroke::new(1.0_f32, LINE));
    });
    ui.add_space(2.0);
}

/// Texto con aberración cromática (efecto glitch): copias desplazadas en UV y coral.
pub fn glitch_title(ui: &mut Ui, text: &str, size: f32) {
    let font = FontId::new(size, FontFamily::Monospace);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font.clone(), SODIUM);
    let (rect, _) =
        ui.allocate_exact_size(galley.size() + Vec2::new(4.0, 0.0), egui::Sense::hover());
    let t = ui.input(|i| i.time);
    // Desplazamiento que "tiembla" de vez en cuando.
    let jitter = if (t * 1.7).fract() < 0.06 { 3.0 } else { 1.5 };
    let p = ui.painter();
    p.text(
        rect.min + Vec2::new(-jitter, 0.0),
        egui::Align2::LEFT_TOP,
        text,
        font.clone(),
        UV.gamma_multiply(0.8),
    );
    p.text(
        rect.min + Vec2::new(jitter, 0.5),
        egui::Align2::LEFT_TOP,
        text,
        font.clone(),
        CORAL.gamma_multiply(0.6),
    );
    p.text(rect.min, egui::Align2::LEFT_TOP, text, font, SODIUM);
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(120));
}

/// Fondo del panel central: rejilla tenue, líneas de barrido y esquinas de "visor".
pub fn hud_background(ui: &Ui, rect: Rect) {
    let p = ui.painter();
    p.rect_filled(rect, Rounding::ZERO, VOID);
    let grid = Stroke::new(1.0_f32, LINE.gamma_multiply(0.35));
    let mut x = rect.left();
    while x < rect.right() {
        p.vline(x, rect.y_range(), grid);
        x += 32.0;
    }
    let mut y = rect.top();
    while y < rect.bottom() {
        p.hline(rect.x_range(), y, grid);
        y += 32.0;
    }
    let scan = Stroke::new(1.0_f32, Color32::from_black_alpha(60));
    let mut y = rect.top();
    while y < rect.bottom() {
        p.hline(rect.x_range(), y, scan);
        y += 3.0;
    }
    corners(ui, rect.shrink(4.0), SODIUM, 14.0);
}

/// Esquinas en "L" alrededor de un rectángulo.
pub fn corners(ui: &Ui, r: Rect, color: Color32, len: f32) {
    let s = Stroke::new(2.0_f32, color);
    let p = ui.painter();
    for (c, dx, dy) in [
        (r.left_top(), 1.0, 1.0),
        (r.right_top(), -1.0, 1.0),
        (r.left_bottom(), 1.0, -1.0),
        (r.right_bottom(), -1.0, -1.0),
    ] {
        p.line_segment([c, c + Vec2::new(dx * len, 0.0)], s);
        p.line_segment([c, c + Vec2::new(0.0, dy * len)], s);
    }
}

/// Botón con borde de color y relleno translúcido.
pub fn neon_button(text: &str, color: Color32) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text.to_owned()).color(color).strong())
        .fill(color.gamma_multiply(0.08))
        .stroke(Stroke::new(1.0_f32, color))
}
