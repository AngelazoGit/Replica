//! Máscaras visuales de Replica: el mismo sistema que CAPAS (barra lateral,
//! cabecera, tarjetas, iconos de línea, Inter + JetBrains Mono) con sus cinco
//! paletas, más «Sodio», la estética original de Replica.

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId,
    Layout, Margin, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy)]
pub struct Paleta {
    pub fondo: Color32,
    pub panel: Color32,
    pub borde: Color32,
    pub medio: Color32,
    pub suave: Color32,
    pub verde: Color32,
    pub verde_oscuro: Color32,
    pub acento: Color32,
    pub naranja: Color32,
    pub naranja_oscuro: Color32,
    pub rojo: Color32,
    pub texto: Color32,
    pub hondo: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

const SERIO: Paleta = Paleta {
    fondo: rgb(0x0f, 0x11, 0x17),
    panel: rgb(0x19, 0x1c, 0x26),
    borde: rgb(0x26, 0x2a, 0x37),
    medio: rgb(0x35, 0x3b, 0x4d),
    suave: rgb(0x9a, 0xa3, 0xb7),
    verde: rgb(0x35, 0xd4, 0x9a),
    verde_oscuro: rgb(0x16, 0x35, 0x2a),
    acento: rgb(0x6d, 0x5c, 0xf5),
    naranja: rgb(0xf0, 0xb8, 0x4e),
    naranja_oscuro: rgb(0x3a, 0x2f, 0x17),
    rojo: rgb(0xef, 0x5f, 0x6b),
    texto: rgb(0xe9, 0xeb, 0xf2),
    hondo: rgb(0x0b, 0x0d, 0x12),
};
const LIMPIO: Paleta = Paleta {
    fondo: rgb(0xf5, 0xf5, 0xf7),
    panel: rgb(0xff, 0xff, 0xff),
    borde: rgb(0xe7, 0xe7, 0xea),
    medio: rgb(0xbf, 0xbf, 0xc8),
    suave: rgb(0x44, 0x47, 0x4f),
    verde: rgb(0x2f, 0x8f, 0x52),
    verde_oscuro: rgb(0xd6, 0xea, 0xd9),
    acento: rgb(0x2f, 0x8f, 0x52),
    naranja: rgb(0xb0, 0x71, 0x16),
    naranja_oscuro: rgb(0xf0, 0xe1, 0xc6),
    rojo: rgb(0xc2, 0x3b, 0x2e),
    texto: rgb(0x1a, 0x1b, 0x1f),
    hondo: rgb(0xff, 0xff, 0xff),
};
const SERENO: Paleta = Paleta {
    fondo: rgb(0x19, 0x14, 0x29),
    panel: rgb(0x23, 0x1c, 0x3a),
    borde: rgb(0x30, 0x26, 0x50),
    medio: rgb(0x5b, 0x4c, 0x85),
    suave: rgb(0xb6, 0xa4, 0xe2),
    verde: rgb(0x86, 0xc9, 0x6e),
    verde_oscuro: rgb(0x3c, 0x5b, 0x34),
    acento: rgb(0x86, 0xc9, 0x6e),
    naranja: rgb(0xe6, 0xa1, 0x5a),
    naranja_oscuro: rgb(0x7e, 0x56, 0x2e),
    rojo: rgb(0xdf, 0x6e, 0x7c),
    texto: rgb(0xec, 0xe8, 0xf6),
    hondo: rgb(0x13, 0x0f, 0x20),
};
const NEUTRO: Paleta = Paleta {
    fondo: rgb(0x1e, 0x1e, 0x22),
    panel: rgb(0x28, 0x28, 0x2e),
    borde: rgb(0x33, 0x33, 0x3b),
    medio: rgb(0x5a, 0x5a, 0x66),
    suave: rgb(0xb8, 0xb8, 0xc4),
    verde: rgb(0x8f, 0xb9, 0x8a),
    verde_oscuro: rgb(0x44, 0x53, 0x3f),
    acento: rgb(0x8f, 0xb9, 0x8a),
    naranja: rgb(0xcf, 0xa8, 0x78),
    naranja_oscuro: rgb(0x6b, 0x5a, 0x42),
    rojo: rgb(0xc9, 0x8a, 0x8a),
    texto: rgb(0xe8, 0xe8, 0xec),
    hondo: rgb(0x16, 0x16, 0x19),
};
const MANDO: Paleta = Paleta {
    fondo: rgb(0x0f, 0x07, 0x1c),
    panel: rgb(0x1a, 0x0b, 0x2e),
    borde: rgb(0x2e, 0x14, 0x52),
    medio: rgb(0x55, 0x2a, 0x93),
    suave: rgb(0x9b, 0x6b, 0xe3),
    verde: rgb(0x8c, 0xff, 0x3c),
    verde_oscuro: rgb(0x2b, 0x6e, 0x1e),
    acento: rgb(0x8c, 0xff, 0x3c),
    naranja: rgb(0xff, 0x8a, 0x1c),
    naranja_oscuro: rgb(0x8a, 0x3a, 0x05),
    rojo: rgb(0xff, 0x2e, 0x4a),
    texto: rgb(0xe9, 0xdd, 0xff),
    hondo: rgb(0x08, 0x04, 0x10),
};
/// La estética original de Replica: ámbar de farola de sodio + violeta UV + fósforo.
const SODIO: Paleta = Paleta {
    fondo: rgb(0x0a, 0x09, 0x10),
    panel: rgb(0x11, 0x0f, 0x1a),
    borde: rgb(0x2e, 0x28, 0x44),
    medio: rgb(0x4a, 0x42, 0x66),
    suave: rgb(0xa4, 0x9c, 0xbb),
    verde: rgb(0x4d, 0xff, 0xb8),
    verde_oscuro: rgb(0x14, 0x3d, 0x2e),
    acento: rgb(0xff, 0x9e, 0x3d),
    naranja: rgb(0xff, 0x9e, 0x3d),
    naranja_oscuro: rgb(0x4a, 0x2c, 0x10),
    rojo: rgb(0xff, 0x4f, 0x6e),
    texto: rgb(0xe9, 0xe3, 0xd3),
    hondo: rgb(0x07, 0x06, 0x0b),
};

/// Máscaras disponibles, en el orden del selector (índice = `Config::tema`).
pub const MASCARAS: [(&str, Paleta); 6] = [
    ("Serio", SERIO),
    ("Limpio", LIMPIO),
    ("Sereno", SERENO),
    ("Neutro", NEUTRO),
    ("Mando", MANDO),
    ("Sodio", SODIO),
];

static TEMA: AtomicU8 = AtomicU8::new(0);

pub fn pal() -> Paleta {
    MASCARAS[(TEMA.load(Ordering::Relaxed) as usize).min(MASCARAS.len() - 1)].1
}

pub fn tema() -> u8 {
    TEMA.load(Ordering::Relaxed)
}

pub fn set_tema(ctx: &egui::Context, i: u8) {
    TEMA.store(i.min(MASCARAS.len() as u8 - 1), Ordering::Relaxed);
    aplicar_visuals(ctx);
}

pub fn titulo(t: f32) -> FontId {
    FontId::new(t, FontFamily::Name("titulo".into()))
}
pub fn mono(t: f32) -> FontId {
    FontId::new(t, FontFamily::Monospace)
}
pub fn prop(t: f32) -> FontId {
    FontId::proportional(t)
}

/// Fuente CJK del sistema para chino, japonés o coreano: (bytes, índice de la cara).
/// No se incrusta en el ejecutable (pesaría decenas de MB); se usa la que trae el sistema.
fn fuente_cjk(code: &str) -> Option<(Vec<u8>, u32)> {
    let win = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into()) + "\\Fonts\\";
    let propias: &[&str] = match code {
        "zh" => &["msyh.ttc", "simsun.ttc"],
        "ja" => &["YuGothM.ttc", "meiryo.ttc", "msgothic.ttc"],
        _ => &["malgun.ttf", "gulim.ttc"],
    };
    let mut rutas: Vec<String> = propias.iter().map(|f| format!("{win}{f}")).collect();
    rutas.extend(
        [
            // macOS
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/System/Library/Fonts/AppleSDGothicNeo.ttc",
            // Linux
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        ]
        .map(String::from),
    );
    rutas
        .iter()
        .find_map(|r| std::fs::read(r).ok().map(|b| (b, 0)))
}

/// Carga las fuentes, con una fuente CJK del sistema detrás si la hay;
/// devuelve false si el idioma la necesita (chino, japonés, coreano) y no hay ninguna.
pub fn cargar_fuentes(ctx: &egui::Context, idioma: &str) -> bool {
    // Siempre que el sistema tenga una, para que «中文», «日本語»… se vean en el selector.
    let cjk = fuente_cjk(if crate::i18n::es_cjk(idioma) {
        idioma
    } else {
        "zh"
    });
    let falta = crate::i18n::es_cjk(idioma) && cjk.is_none();
    ctx.set_fonts(definiciones(cjk));
    !falta
}

pub fn configurar_estilo(ctx: &egui::Context, tema: u8, idioma: &str) {
    cargar_fuentes(ctx, idioma);
    TEMA.store(tema.min(MASCARAS.len() as u8 - 1), Ordering::Relaxed);
    aplicar_visuals(ctx);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(6.0, 4.0);
        s.spacing.button_padding = Vec2::new(10.0, 5.0);
    });
}

fn definiciones(cjk: Option<(Vec<u8>, u32)>) -> FontDefinitions {
    // Fuentes embebidas: se ven idénticas en Windows, Linux y macOS.
    const INTER: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.otf");
    const INTER_SB: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.otf");
    const INTER_BOLD: &[u8] = include_bytes!("../assets/fonts/Inter-Bold.otf");
    const JBMONO: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");

    let mut f = FontDefinitions::default();
    f.font_data
        .insert("inter".into(), Arc::new(FontData::from_static(INTER)));
    f.font_data
        .insert("inter_sb".into(), Arc::new(FontData::from_static(INTER_SB)));
    f.font_data.insert(
        "inter_bold".into(),
        Arc::new(FontData::from_static(INTER_BOLD)),
    );
    f.font_data
        .insert("jbmono".into(), Arc::new(FontData::from_static(JBMONO)));
    // Se conservan detrás las de egui (emoji y símbolos).
    let resp_prop = f
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let resp_mono = f
        .families
        .get(&FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();
    let mut p = vec!["inter".to_string()];
    p.extend(resp_prop.iter().cloned());
    f.families.insert(FontFamily::Proportional, p);
    let mut t = vec!["inter_bold".to_string(), "inter_sb".to_string()];
    t.extend(resp_prop.iter().cloned());
    f.families.insert(FontFamily::Name("titulo".into()), t);
    let mut m = vec!["jbmono".to_string()];
    m.extend(resp_mono.iter().cloned());
    f.families.insert(FontFamily::Monospace, m);
    if let Some((bytes, index)) = cjk {
        let mut d = FontData::from_owned(bytes);
        d.index = index;
        f.font_data.insert("cjk".into(), Arc::new(d));
        // Detrás de Inter/JetBrains: solo se usa para los caracteres que ellas no tienen.
        for fam in [
            FontFamily::Proportional,
            FontFamily::Monospace,
            FontFamily::Name("titulo".into()),
        ] {
            f.families.entry(fam).or_default().push("cjk".into());
        }
    }
    f
}

fn aplicar_visuals(ctx: &egui::Context) {
    let c = pal();
    let clara = c.fondo.r() > 200;
    let mut v = if clara {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    v.override_text_color = Some(c.texto);
    v.panel_fill = c.fondo;
    v.window_fill = c.panel;
    v.extreme_bg_color = c.hondo;
    v.faint_bg_color = c.panel;
    v.selection.bg_fill = c.verde_oscuro;
    v.selection.stroke = Stroke::new(1.0, c.acento);
    v.hyperlink_color = c.acento;
    v.window_corner_radius = CornerRadius::same(10);
    v.window_stroke = Stroke::new(1.0, c.borde);
    for (w, fondo, borde) in [
        (&mut v.widgets.noninteractive, c.panel, c.borde),
        (&mut v.widgets.inactive, c.borde, c.medio),
        (&mut v.widgets.hovered, c.medio, c.acento),
        (&mut v.widgets.active, c.naranja_oscuro, c.naranja),
        (&mut v.widgets.open, c.medio, c.naranja),
    ] {
        w.bg_fill = fondo;
        w.weak_bg_fill = fondo;
        w.bg_stroke = Stroke::new(1.0, borde);
        w.corner_radius = CornerRadius::same(5);
    }
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, c.acento);
    v.widgets.active.fg_stroke = Stroke::new(1.5, c.naranja);
    v.slider_trailing_fill = true;
    ctx.set_visuals(v);
}

pub fn mezclar(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

// ── Componentes ─────────────────────────────────────────────────────────

/// Elemento de la barra lateral; devuelve true al pulsarlo.
pub fn nav_item(ui: &mut Ui, icono: &str, etiqueta: &str, ayuda: &str, activo: bool) -> bool {
    let c = pal();
    let alto = 42.0;
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), alto), Sense::click());
    let resp = resp.on_hover_text(ayuda);
    let p = ui.painter().clone();
    if activo {
        p.rect_filled(rect, CornerRadius::same(9), c.acento.gamma_multiply(0.20));
        p.rect_filled(
            Rect::from_min_size(rect.left_top(), Vec2::new(3.0, alto)),
            CornerRadius::same(2),
            c.acento,
        );
    } else if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(9), c.borde.gamma_multiply(0.6));
    }
    let icol = if activo { c.acento } else { c.suave };
    dibujar_icono(
        &p,
        Pos2::new(rect.left() + 22.0, rect.center().y),
        8.0,
        icono,
        icol,
    );
    let fuente = if activo { titulo(14.5) } else { prop(14.5) };
    p.text(
        Pos2::new(rect.left() + 42.0, rect.center().y),
        Align2::LEFT_CENTER,
        etiqueta,
        fuente,
        if activo { c.texto } else { c.suave },
    );
    ui.add_space(5.0);
    resp.clicked()
}

/// Logo de Replica: dos fichas superpuestas (original y copia) con un «play».
pub fn marca(p: &egui::Painter, centro: Pos2, s: f32, base: Color32) {
    let c = pal();
    let atras = Rect::from_center_size(
        centro + Vec2::new(-s * 0.28, -s * 0.28),
        Vec2::splat(s * 1.5),
    );
    let delante =
        Rect::from_center_size(centro + Vec2::new(s * 0.28, s * 0.28), Vec2::splat(s * 1.5));
    p.rect_filled(
        atras,
        CornerRadius::same((s * 0.3) as u8),
        base.gamma_multiply(0.45),
    );
    p.rect_filled(delante, CornerRadius::same((s * 0.3) as u8), base);
    let k = delante.center();
    p.add(egui::Shape::convex_polygon(
        vec![
            k + Vec2::new(-s * 0.22, -s * 0.36),
            k + Vec2::new(s * 0.36, 0.0),
            k + Vec2::new(-s * 0.22, s * 0.36),
        ],
        c.hondo,
        Stroke::NONE,
    ));
}

/// Iconos vectoriales de línea. `s` = semitamaño.
pub fn dibujar_icono(p: &egui::Painter, c: Pos2, s: f32, kind: &str, col: Color32) {
    let st = Stroke::new((s * 0.17).max(1.4), col);
    let poli = |pts: Vec<Pos2>| p.add(egui::Shape::line(pts, st));
    let ln = |a: Pos2, b: Pos2| p.line_segment([a, b], st);
    let caja = |min: Pos2, max: Pos2| {
        p.rect_stroke(
            Rect::from_min_max(min, max),
            CornerRadius::same((s * 0.22).max(1.0) as u8),
            st,
            StrokeKind::Middle,
        )
    };
    let g = |x: f32, y: f32| Pos2::new(c.x + x * s, c.y + y * s);
    match kind {
        "home" => {
            poli(vec![g(-1.0, -0.1), g(0.0, -1.0), g(1.0, -0.1)]);
            poli(vec![
                g(-0.72, -0.12),
                g(-0.72, 0.82),
                g(0.72, 0.82),
                g(0.72, -0.12),
            ]);
        }
        "rutas" => {
            // Camino con puntos de paso.
            poli(vec![
                g(-0.85, 0.7),
                g(-0.3, 0.1),
                g(0.25, 0.45),
                g(0.85, -0.6),
            ]);
            p.circle_filled(g(-0.85, 0.7), s * 0.16, col);
            p.circle_filled(g(0.85, -0.6), s * 0.16, col);
        }
        "reloj" => {
            p.circle_stroke(c, s * 0.85, st);
            ln(c, g(0.0, -0.5));
            ln(c, g(0.38, 0.2));
        }
        "registro" | "informe" => {
            poli(vec![
                g(-0.6, -0.82),
                g(0.25, -0.82),
                g(0.6, -0.46),
                g(0.6, 0.82),
                g(-0.6, 0.82),
                g(-0.6, -0.82),
            ]);
            for yy in [-0.3f32, 0.05, 0.4] {
                ln(g(-0.35, yy), g(0.38, yy));
            }
        }
        "engranaje" => {
            p.circle_stroke(c, s * 0.42, st);
            for i in 0..8 {
                let d = Vec2::angled(i as f32 * std::f32::consts::TAU / 8.0);
                ln(c + d * (s * 0.55), c + d * (s * 0.92));
            }
        }
        "grabar" => {
            p.circle_stroke(c, s * 0.85, st);
            p.circle_filled(c, s * 0.42, col);
        }
        "play" => {
            p.add(egui::Shape::convex_polygon(
                vec![g(-0.45, -0.7), g(0.7, 0.0), g(-0.45, 0.7)],
                Color32::TRANSPARENT,
                st,
            ));
        }
        "paso" => {
            p.add(egui::Shape::convex_polygon(
                vec![g(-0.7, -0.6), g(0.2, 0.0), g(-0.7, 0.6)],
                Color32::TRANSPARENT,
                st,
            ));
            ln(g(0.55, -0.6), g(0.55, 0.6));
        }
        "teclado" => {
            caja(g(-0.92, -0.55), g(0.92, 0.55));
            for x in [-0.5f32, 0.0, 0.5] {
                p.circle_filled(g(x, -0.15), s * 0.09, col);
            }
            ln(g(-0.4, 0.25), g(0.4, 0.25));
        }
        "ojo" => {
            poli(vec![
                g(-0.85, 0.0),
                g(0.0, -0.55),
                g(0.85, 0.0),
                g(0.0, 0.55),
                g(-0.85, 0.0),
            ]);
            p.circle_filled(c, s * 0.22, col);
        }
        "carpeta" => {
            poli(vec![
                g(-0.85, 0.62),
                g(-0.85, -0.42),
                g(-0.18, -0.42),
                g(0.0, -0.2),
                g(0.85, -0.2),
                g(0.85, 0.62),
                g(-0.85, 0.62),
            ]);
        }
        "aviso" => {
            poli(vec![
                g(0.0, -0.8),
                g(0.85, 0.65),
                g(-0.85, 0.65),
                g(0.0, -0.8),
            ]);
            ln(g(0.0, -0.12), g(0.0, 0.28));
            p.circle_filled(g(0.0, 0.5), s * 0.1, col);
        }
        "monitor" => {
            caja(g(-0.9, -0.7), g(0.9, 0.45));
            ln(g(-0.35, 0.82), g(0.35, 0.82));
            ln(g(0.0, 0.45), g(0.0, 0.82));
        }
        "check" => {
            p.circle_stroke(c, s * 0.9, st);
            poli(vec![g(-0.4, 0.0), g(-0.1, 0.32), g(0.42, -0.3)]);
        }
        _ => {
            p.circle_stroke(c, s * 0.7, st);
        }
    }
}

/// Botón principal (relleno).
pub fn boton_pri(texto: &str, color: Color32) -> egui::Button<'static> {
    let c = pal();
    let claro = color == c.acento || color == c.rojo;
    let fg = if claro && c.fondo.r() < 200 || color == c.rojo {
        rgb(0xf6, 0xf7, 0xfb)
    } else {
        rgb(0x12, 0x14, 0x1a)
    };
    egui::Button::new(
        RichText::new(texto.to_string())
            .font(titulo(13.5))
            .color(fg),
    )
    .fill(color)
    .corner_radius(CornerRadius::same(8))
    .min_size(Vec2::new(0.0, 36.0))
}

/// Botón secundario (contorno).
pub fn boton_sec(texto: &str) -> egui::Button<'static> {
    let c = pal();
    egui::Button::new(
        RichText::new(texto.to_string())
            .font(prop(12.5))
            .color(c.texto),
    )
    .fill(c.borde)
    .stroke(Stroke::new(1.0, c.medio))
    .corner_radius(CornerRadius::same(7))
    .min_size(Vec2::new(0.0, 32.0))
}

/// Botón pequeño de barra de herramientas.
pub fn boton_mini(texto: &str) -> egui::Button<'static> {
    let c = pal();
    egui::Button::new(
        RichText::new(texto.to_string())
            .font(prop(12.0))
            .color(c.texto),
    )
    .fill(c.borde)
    .stroke(Stroke::new(1.0, c.medio))
    .corner_radius(CornerRadius::same(6))
    .min_size(Vec2::new(0.0, 26.0))
}

/// Marco de tarjeta.
pub fn card() -> egui::Frame {
    let c = pal();
    egui::Frame::new()
        .fill(c.panel)
        .stroke(Stroke::new(1.0, c.borde))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(14))
}

/// Tarjeta con título.
pub fn tarjeta<R>(ui: &mut Ui, titulo_txt: &str, contenido: impl FnOnce(&mut Ui) -> R) -> R {
    card()
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(titulo_txt)
                    .font(titulo(15.0))
                    .color(pal().texto),
            );
            ui.add_space(8.0);
            contenido(ui)
        })
        .inner
}

/// Tarjeta de estadística con barra de color a la izquierda.
pub fn stat_card(ui: &mut Ui, etiqueta: &str, valor: &str, sub: &str, color: Color32) {
    let c = pal();
    let inner = egui::Frame::new()
        .fill(c.panel)
        .stroke(Stroke::new(1.0, c.borde))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin {
            left: 18,
            right: 14,
            top: 9,
            bottom: 10,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(etiqueta.to_uppercase())
                    .font(mono(10.0))
                    .color(c.suave),
            );
            ui.label(RichText::new(valor).font(titulo(28.0)).color(c.texto));
            ui.label(RichText::new(sub).font(prop(11.5)).color(c.suave));
        });
    let rr = inner.response.rect;
    ui.painter().rect_filled(
        Rect::from_min_size(
            rr.left_top() + Vec2::new(0.0, 8.0),
            Vec2::new(3.0, rr.height() - 16.0),
        ),
        CornerRadius::same(2),
        color,
    );
}

/// Tarjeta de acceso clicable. Devuelve true al pulsarla.
pub fn tarjeta_accion(ui: &mut Ui, icono: &str, titulo_txt: &str, desc: &str) -> bool {
    let c = pal();
    let id = ui.make_persistent_id(("accion", titulo_txt));
    let hov = ui
        .ctx()
        .read_response(id)
        .map(|r| r.hovered())
        .unwrap_or(false);
    let inner = egui::Frame::new()
        .fill(if hov {
            mezclar(c.panel, c.borde, 0.5)
        } else {
            c.panel
        })
        .stroke(Stroke::new(1.0, if hov { c.acento } else { c.borde }))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(104.0);
            let (badge, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
            let p = ui.painter().clone();
            p.rect_filled(badge, CornerRadius::same(8), c.acento.gamma_multiply(0.18));
            dibujar_icono(&p, badge.center(), 9.0, icono, c.acento);
            ui.add_space(9.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(titulo_txt).font(titulo(14.5)).color(c.texto));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("›").font(titulo(17.0)).color(if hov {
                        c.acento
                    } else {
                        c.medio
                    }));
                });
            });
            ui.add_space(2.0);
            ui.label(RichText::new(desc).font(prop(12.0)).color(c.suave));
        });
    ui.interact(inner.response.rect, id, Sense::click())
        .clicked()
}

/// Etiqueta tipo chip con contorno.
pub fn chip(ui: &mut Ui, t: &str, col: Color32) {
    let g = ui
        .painter()
        .layout_no_wrap(t.to_string(), titulo(11.5), col);
    let (resp, p) = ui.allocate_painter(g.size() + Vec2::new(14.0, 6.0), Sense::hover());
    p.rect_filled(resp.rect, CornerRadius::same(4), col.gamma_multiply(0.20));
    p.rect_stroke(
        resp.rect,
        CornerRadius::same(4),
        Stroke::new(1.0, col),
        StrokeKind::Inside,
    );
    p.galley(resp.rect.center() - g.size() / 2.0, g, col);
}

/// Selector tipo «píldoras» (como el de máscaras de CAPAS). Devuelve true si cambia.
pub fn pildoras<T: PartialEq + Copy>(ui: &mut Ui, actual: &mut T, opciones: &[(T, &str)]) -> bool {
    let c = pal();
    let mut cambio = false;
    ui.horizontal_wrapped(|ui| {
        for (v, txt) in opciones {
            let sel = *actual == *v;
            let b = egui::Button::new(RichText::new(*txt).font(prop(12.5)).color(if sel {
                rgb(0xf6, 0xf7, 0xfb)
            } else {
                c.texto
            }))
            .fill(if sel { c.acento } else { c.borde })
            .stroke(Stroke::new(1.0, if sel { c.acento } else { c.medio }))
            .corner_radius(CornerRadius::same(6))
            .min_size(Vec2::new(0.0, 30.0));
            if ui.add(b).clicked() && !sel {
                *actual = *v;
                cambio = true;
            }
        }
    });
    cambio
}

/// Cabecera de una vista (icono + título + subtítulo).
pub fn titulo_vista(ui: &mut Ui, icono: &str, titulo_txt: &str, sub: &str) {
    let c = pal();
    ui.horizontal(|ui| {
        let (rid, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
        dibujar_icono(&ui.painter().clone(), rid.center(), 9.5, icono, c.acento);
        ui.add_space(2.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(titulo_txt).font(titulo(19.0)).color(c.texto));
            ui.label(RichText::new(sub).font(prop(12.0)).color(c.suave));
        });
    });
    ui.add_space(12.0);
}

/// Estado vacío centrado con icono grande.
pub fn panel_vacio(ui: &mut Ui, icono: &str, titulo_txt: &str, sub: &str) {
    let c = pal();
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        let (rid, _) = ui.allocate_exact_size(Vec2::splat(50.0), Sense::hover());
        dibujar_icono(&ui.painter().clone(), rid.center(), 19.0, icono, c.medio);
        ui.add_space(10.0);
        ui.label(RichText::new(titulo_txt).font(titulo(17.0)).color(c.suave));
        ui.label(RichText::new(sub).font(prop(12.5)).color(c.medio));
    });
}

/// Encabezado fino de un panel.
pub fn encabezado_panel(ui: &mut Ui, t: &str) {
    let c = pal();
    ui.horizontal(|ui| {
        let (rid, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
        ui.painter().rect_filled(
            Rect::from_center_size(rid.center(), Vec2::splat(7.0)),
            CornerRadius::same(2),
            c.acento,
        );
        ui.label(RichText::new(t).font(titulo(13.0)).color(c.texto));
    });
    ui.add_space(6.0);
    let y = ui.cursor().top();
    ui.painter().line_segment(
        [
            Pos2::new(ui.max_rect().left(), y),
            Pos2::new(ui.max_rect().right(), y),
        ],
        Stroke::new(1.0, c.borde),
    );
    ui.add_space(8.0);
}

/// Etiqueta pequeña de ayuda bajo un ajuste.
pub fn ayuda(ui: &mut Ui, t: &str) {
    ui.label(RichText::new(t).font(prop(11.5)).color(pal().suave));
}

/// Icono de la ventana: el logo sobre el fondo de la máscara Serio.
pub fn icono_app() -> egui::IconData {
    let n = 64usize;
    let mut rgba = vec![0u8; n * n * 4];
    let base = SERIO.acento;
    for y in 0..n {
        for x in 0..n {
            let (fx, fy) = (x as f32, y as f32);
            let en = |x0: f32, y0: f32, w: f32| fx >= x0 && fx < x0 + w && fy >= y0 && fy < y0 + w;
            let px = if en(28.0, 28.0, 30.0) {
                // «play» recortado en la ficha delantera
                let (u, v) = (fx - 36.0, fy - 43.0);
                if u >= 0.0 && u < 15.0 && v.abs() < 8.0 - u * 0.53 {
                    SERIO.hondo
                } else {
                    base
                }
            } else if en(6.0, 6.0, 30.0) {
                base.gamma_multiply(0.45)
            } else {
                Color32::TRANSPARENT
            };
            let i = (y * n + x) * 4;
            rgba[i..i + 4].copy_from_slice(&px.to_array());
        }
    }
    egui::IconData {
        rgba,
        width: n as u32,
        height: n as u32,
    }
}
