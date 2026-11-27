//! Interfaz gráfica de Replica con la máscara de CAPAS: barra lateral, cabecera
//! con estado y acciones, cuerpo en tarjetas y barra de estado.

use crate::bot::{Bot, Mode, ROUTE_HOTKEYS, simplify_moves};
use crate::config::{Schedule, When};
use crate::model::{Action, MissingAnchor, Route, Settings, Step, Target};
use crate::theme::{self, card, chip, mono, pal, prop, titulo};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Layout, Margin, Pos2, Rect, RichText, Sense,
    Stroke, Ui, Vec2,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, PartialEq)]
enum Vista {
    Inicio,
    Rutas,
    Programador,
    Registro,
    Ajustes,
}

impl Vista {
    const TODAS: [Vista; 5] = [
        Vista::Inicio,
        Vista::Rutas,
        Vista::Programador,
        Vista::Registro,
        Vista::Ajustes,
    ];

    fn etiqueta(self) -> &'static str {
        match self {
            Vista::Inicio => "Inicio",
            Vista::Rutas => "Rutas",
            Vista::Programador => "Programador",
            Vista::Registro => "Registro",
            Vista::Ajustes => "Ajustes",
        }
    }
    fn icono(self) -> &'static str {
        match self {
            Vista::Inicio => "home",
            Vista::Rutas => "rutas",
            Vista::Programador => "reloj",
            Vista::Registro => "registro",
            Vista::Ajustes => "engranaje",
        }
    }
    fn ayuda(self) -> &'static str {
        match self {
            Vista::Inicio => "Resumen y accesos rápidos",
            Vista::Rutas => "Grabar, editar y reproducir rutas",
            Vista::Programador => "Lanzar rutas a una hora o cada X minutos",
            Vista::Registro => "Todo lo que ha hecho Replica",
            Vista::Ajustes => "Máscara visual y bandeja del sistema",
        }
    }
}

/// Acción que espera confirmación porque hay cambios sin guardar.
enum Pendiente {
    Cargar(String),
    Cerrar,
}

/// Cambios pedidos desde la lista de pasos (se aplican al terminar de dibujarla).
enum Edicion {
    Cambiar(usize, Step),
    Borrar(usize),
    Subir(usize),
}

pub struct App {
    bot: Arc<Bot>,
    vista: Vista,
    routes: Vec<String>,
    textures: HashMap<PathBuf, egui::TextureHandle>,
    new_text: String,
    new_cmd: String,
    new_wait: u64,
    last_mode: Mode,
    was_saving: bool,
    windows: Vec<Target>,
    /// Estados anteriores de la ruta para deshacer (Ctrl+Z).
    undo: Vec<Route>,
    /// La ruta y ajustes tal como están guardados: para saber si hay cambios.
    snap: (Route, Settings),
    pendiente: Option<Pendiente>,
    /// Fila editada por última vez (para agrupar el «deshacer»).
    ultima_fila: Option<usize>,
    /// El usuario confirmó salir (o eligió «Salir» en la bandeja).
    pub quit: Arc<AtomicBool>,
    /// Icono de la bandeja (Windows): se conserva aquí para que no desaparezca.
    pub _tray: Option<Box<dyn std::any::Any>>,
    // Formulario del programador.
    f_route: String,
    f_daily: bool,
    f_hour: u32,
    f_min: u32,
    f_every: u32,
}

impl App {
    pub fn new(cc: &eframe::CreationContext, bot: Arc<Bot>, quit: Arc<AtomicBool>) -> Self {
        bot.set_repaint(cc.egui_ctx.clone());
        let snap = {
            let st = bot.state.lock().unwrap();
            (st.route.clone(), st.settings.clone())
        };
        Self {
            bot,
            vista: Vista::Inicio,
            routes: Route::list(),
            textures: HashMap::new(),
            new_text: "/home".into(),
            new_cmd: String::new(),
            new_wait: 1000,
            last_mode: Mode::Idle,
            was_saving: false,
            windows: vec![],
            undo: vec![],
            snap,
            pendiente: None,
            ultima_fila: None,
            quit,
            _tray: None,
            f_route: String::new(),
            f_daily: true,
            f_hour: 9,
            f_min: 0,
            f_every: 30,
        }
    }

    fn anchor_texture(
        &mut self,
        ctx: &egui::Context,
        path: PathBuf,
    ) -> Option<egui::TextureHandle> {
        if let Some(tex) = self.textures.get(&path) {
            return Some(tex.clone());
        }
        // Sin caché si falta: el hilo de anclas puede escribir el PNG un instante después.
        let img = image::open(&path).ok()?.to_rgba8();
        let size = [img.width() as usize, img.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
        let tex = ctx.load_texture(path.to_string_lossy(), color, Default::default());
        self.textures.insert(path, tex.clone());
        Some(tex)
    }

    fn dirty(&self) -> bool {
        let st = self.bot.state.lock().unwrap();
        st.route != self.snap.0 || st.settings != self.snap.1
    }

    fn resnap(&mut self) {
        let st = self.bot.state.lock().unwrap();
        self.snap = (st.route.clone(), st.settings.clone());
        drop(st);
        self.undo.clear();
    }

    /// Aplica un cambio a la ruta guardando el estado anterior para deshacer.
    fn edit(&mut self, f: impl FnOnce(&mut Route)) {
        let mut st = self.bot.state.lock().unwrap();
        self.undo.push(st.route.clone());
        if self.undo.len() > 50 {
            self.undo.remove(0);
        }
        f(&mut st.route);
        drop(st);
        self.ultima_fila = None;
    }

    fn deshacer(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.bot.state.lock().unwrap().route = prev;
            self.bot.log("Deshecho el último cambio.");
        }
    }

    fn guardar(&mut self) {
        match self.bot.save_route() {
            Ok(()) => {
                let name = self.bot.state.lock().unwrap().route.name.clone();
                self.bot.log(format!("Ruta '{name}' guardada."));
                self.resnap();
                self.routes = Route::list();
            }
            Err(e) => self.bot.log(format!("Error guardando: {e}")),
        }
    }

    fn cargar(&mut self, name: &str) {
        match self.bot.load_route(name) {
            Ok(()) => {
                self.bot.log(format!("Ruta '{name}' cargada."));
                self.resnap();
                self.textures.clear();
                self.vista = Vista::Rutas;
            }
            Err(e) => self.bot.log(format!("Error cargando '{name}': {e}")),
        }
    }

    /// Cargar otra ruta, preguntando antes si hay cambios sin guardar.
    fn pedir_cargar(&mut self, name: String) {
        if self.dirty() {
            self.pendiente = Some(Pendiente::Cargar(name));
        } else {
            self.cargar(&name);
        }
    }
}

impl eframe::App for App {
    fn clear_color(&self, _v: &egui::Visuals) -> [f32; 4] {
        pal().fondo.to_normalized_gamma_f32()
    }

    /// Al cerrar: parar y soltar teclas/botones antes de que termine el proceso.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.bot.shutdown();
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let bot = self.bot.clone();
        let mode = bot.mode();
        let saving = bot.is_saving();

        // Tras grabar (cuando ya está guardada) o si un atajo/programación cargó otra ruta.
        let snap_name_changed = bot.state.lock().unwrap().route.name != self.snap.0.name;
        if (self.was_saving && !saving) || (snap_name_changed && mode != Mode::Recording && !saving)
        {
            self.resnap();
            self.routes = Route::list();
        }
        if mode != self.last_mode {
            self.routes = Route::list();
            self.textures.clear();
            self.windows.clear();
            self.last_mode = mode;
        }
        self.was_saving = saving;
        let dirty = mode == Mode::Idle && !saving && self.dirty();
        bot.state.lock().unwrap().unsaved = dirty;

        // Ctrl+Z / Ctrl+S
        if mode == Mode::Idle {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
                self.deshacer();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
                self.guardar();
            }
        }

        // Cerrar la ventana: a la bandeja si está activada; si hay cambios, preguntar.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quit.load(Ordering::SeqCst) {
            let tray = cfg!(windows) && !bot.state.lock().unwrap().config.no_tray;
            if tray {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                bot.log("Replica sigue en la bandeja del sistema (atajos y programador activos).");
            } else if dirty {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pendiente = Some(Pendiente::Cerrar);
            }
        }
        // «Salir» desde la bandeja con cambios sin guardar: mostrar la ventana y preguntar.
        if self.quit.load(Ordering::SeqCst) && dirty && self.pendiente.is_none() {
            self.quit.store(false, Ordering::SeqCst);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            self.pendiente = Some(Pendiente::Cerrar);
        }

        let c = pal();
        let marco_nav = egui::Frame::new().fill(c.hondo).inner_margin(Margin {
            left: 14,
            right: 14,
            top: 16,
            bottom: 12,
        });
        let marco_cab = egui::Frame::new().fill(c.panel).inner_margin(Margin {
            left: 22,
            right: 22,
            top: 0,
            bottom: 0,
        });
        let marco_cuerpo = egui::Frame::new()
            .fill(c.fondo)
            .inner_margin(Margin::same(18));

        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(216.0)
            .frame(marco_nav)
            .show(ui, |ui| self.nav_ui(ui));
        egui::Panel::top("encabezado")
            .exact_size(72.0)
            .frame(marco_cab)
            .show(ui, |ui| self.encabezado_ui(ui, mode, saving));
        egui::Panel::bottom("estado")
            .exact_size(32.0)
            .frame(
                egui::Frame::new()
                    .fill(c.panel)
                    .inner_margin(Margin::symmetric(16, 4)),
            )
            .show(ui, |ui| self.barra_estado_ui(ui, mode, dirty));
        egui::CentralPanel::default()
            .frame(marco_cuerpo)
            .show(ui, |ui| match self.vista {
                Vista::Inicio => self.vista_inicio(ui, mode),
                Vista::Rutas => self.vista_rutas(ui, &ctx, mode, saving, dirty),
                Vista::Programador => self.vista_programador(ui),
                Vista::Registro => self.vista_registro(ui),
                Vista::Ajustes => self.vista_ajustes(ui, &ctx),
            });

        self.dialogo(&ctx);
        if mode != Mode::Idle || saving {
            ctx.request_repaint_after(std::time::Duration::from_millis(50)); // pulso de la insignia
        }
    }
}

// ── Estructura ──────────────────────────────────────────────────────────
impl App {
    fn nav_ui(&mut self, ui: &mut Ui) {
        let c = pal();
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::hover());
        let p = ui.painter().clone();
        theme::marca(
            &p,
            Pos2::new(rect.left() + 16.0, rect.center().y + 1.0),
            11.0,
            c.acento,
        );
        p.text(
            Pos2::new(rect.left() + 40.0, rect.center().y),
            Align2::LEFT_CENTER,
            "Replica",
            titulo(25.0),
            c.texto,
        );
        ui.add_space(2.0);
        ui.label(
            RichText::new("Graba lo que haces y lo\nrepite por ti, mirando la pantalla")
                .font(prop(10.5))
                .color(c.suave),
        );
        ui.add_space(14.0);
        let yd = ui.cursor().top();
        ui.painter().line_segment(
            [
                Pos2::new(ui.max_rect().left(), yd),
                Pos2::new(ui.max_rect().right(), yd),
            ],
            Stroke::new(1.0, c.borde),
        );
        ui.add_space(12.0);

        for v in Vista::TODAS {
            if theme::nav_item(ui, v.icono(), v.etiqueta(), v.ayuda(), self.vista == v) {
                self.vista = v;
            }
        }

        // Abajo: próxima ejecución programada.
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            let st = self.bot.state.lock().unwrap();
            let activas: Vec<&Schedule> =
                st.config.schedules.iter().filter(|s| s.enabled).collect();
            let (txt, col) = match activas.first() {
                Some(s) if activas.len() == 1 => (format!("{} · {}", s.route, s.label()), c.verde),
                Some(_) => (format!("{} programaciones activas", activas.len()), c.verde),
                None => ("Sin programaciones".into(), c.medio),
            };
            drop(st);
            ui.label(RichText::new(txt).font(prop(11.0)).color(c.suave));
            ui.horizontal(|ui| {
                let (rid, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                ui.painter().circle_filled(rid.center(), 4.0, col);
                ui.label(RichText::new("Programador").font(titulo(12.5)).color(col));
            });
        });
    }

    fn encabezado_ui(&mut self, ui: &mut Ui, mode: Mode, saving: bool) {
        let c = pal();
        let ahora = ui.input(|i| i.time);
        let bot = self.bot.clone();
        let paused = bot.is_paused();
        let (play, route_name, steps, rec_steps) = {
            let st = bot.state.lock().unwrap();
            (
                st.play.clone(),
                st.route.name.clone(),
                st.route.steps.len(),
                st.route.steps.len(),
            )
        };
        ui.horizontal_centered(|ui| {
            // Insignia de estado.
            let (badge, _) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::hover());
            let p = ui.painter().clone();
            let pulso = ((ahora * 3.0).sin() * 0.5 + 0.5) as f32;
            match mode {
                Mode::Recording => {
                    p.circle_filled(badge.center(), 8.0 + 4.0 * pulso, c.rojo.gamma_multiply(0.45));
                    p.circle_filled(badge.center(), 6.0, c.rojo);
                }
                Mode::Playing if play.countdown > 0 => {
                    p.circle_filled(badge.center(), 13.0, c.naranja);
                    p.text(badge.center(), Align2::CENTER_CENTER, play.countdown.to_string(), titulo(15.0), c.hondo);
                }
                Mode::Playing if paused => {
                    p.circle_stroke(badge.center(), 12.0, Stroke::new(2.0, c.naranja));
                    for dx in [-3.0, 3.0] {
                        p.rect_filled(Rect::from_center_size(badge.center() + Vec2::new(dx, 0.0), Vec2::new(3.0, 10.0)), 1.0, c.naranja);
                    }
                }
                Mode::Playing => {
                    p.circle_filled(badge.center(), 7.0 + 3.0 * pulso, c.verde.gamma_multiply(0.5));
                    p.circle_filled(badge.center(), 5.0, c.verde);
                }
                Mode::Idle => {
                    p.circle_stroke(badge.center(), 11.0, Stroke::new(2.0, c.medio));
                }
            }
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.add_space(13.0);
                ui.label(RichText::new(format!("REPLICA   ›   {}", self.vista.etiqueta())).font(prop(11.0)).color(c.suave));
                let t = match mode {
                    Mode::Recording => format!("Grabando… {rec_steps} pasos"),
                    Mode::Playing if play.countdown > 0 => format!("Empieza en {}…", play.countdown),
                    Mode::Playing if paused => "En pausa".to_string(),
                    Mode::Playing if play.steps > 0 => {
                        let laps = if play.laps == 0 { "∞".to_string() } else { play.laps.to_string() };
                        format!("Reproduciendo · vuelta {}/{laps}", play.lap)
                    }
                    Mode::Playing => "Ejecutando…".to_string(),
                    Mode::Idle if saving => "Guardando la grabación…".to_string(),
                    Mode::Idle if route_name.is_empty() => "Listo para grabar".to_string(),
                    Mode::Idle => format!("Listo · {route_name}"),
                };
                ui.label(RichText::new(t).font(titulo(22.0)).color(c.texto));
            });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| match mode {
                Mode::Playing => {
                    if ui.add(theme::boton_pri("■  Parar  F12", c.rojo)).clicked() {
                        bot.stop_all();
                    }
                    let t = if paused { "▶  Seguir  F11" } else { "❚❚  Pausa  F11" };
                    if ui.add(theme::boton_sec(t)).clicked() {
                        bot.toggle_pause();
                    }
                }
                Mode::Recording => {
                    if ui.add(theme::boton_pri("■  Terminar grabación  F9", c.rojo)).clicked() {
                        bot.toggle_record(true);
                    }
                }
                Mode::Idle => {
                    let listo = steps > 0 && !saving;
                    if ui
                        .add_enabled(listo, theme::boton_pri("▶  Reproducir  F10", c.acento))
                        .on_hover_text("Repite la ruta cargada con sus ajustes (cuenta atrás incluida)")
                        .clicked()
                    {
                        bot.toggle_play();
                    }
                    if ui
                        .add_enabled(!saving, theme::boton_pri("●  Grabar  F9", c.rojo))
                        .on_hover_text("Empieza a grabar. Mejor con F9 desde el juego: así no se graba el clic en Replica")
                        .clicked()
                    {
                        bot.toggle_record(true);
                        self.vista = Vista::Rutas;
                    }
                }
            });
        });
        // Línea inferior; mientras reproduce, es la barra de progreso de la vuelta.
        let r = ui.max_rect();
        let (x0, x1, y) = (r.left() - 22.0, r.right() + 22.0, r.bottom() - 1.0);
        ui.painter().line_segment(
            [Pos2::new(x0, y), Pos2::new(x1, y)],
            Stroke::new(1.0, c.borde),
        );
        if mode == Mode::Playing && play.steps > 0 {
            let xf = x0 + (x1 - x0) * play.frac();
            let col = if paused { c.naranja } else { c.acento };
            ui.painter().rect_filled(
                Rect::from_min_max(Pos2::new(x0, y - 2.0), Pos2::new(xf, y + 1.0)),
                0.0,
                col,
            );
        }
    }

    fn barra_estado_ui(&mut self, ui: &mut Ui, mode: Mode, dirty: bool) {
        let c = pal();
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (rid, _) = ui.allocate_exact_size(Vec2::splat(9.0), Sense::hover());
            ui.painter().circle_filled(rid.center(), 4.0, c.verde);
            ui.label(
                RichText::new("Entrada global activa")
                    .font(prop(11.5))
                    .color(c.suave),
            );
            ui.label(RichText::new("·").font(prop(11.5)).color(c.medio));
            ui.label(
                RichText::new(
                    "F9 grabar · F10 reproducir · F11 pausa · F12 parar · F1–F8 atajos de ruta",
                )
                .font(mono(11.0))
                .color(c.medio),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (name, n) = {
                    let st = self.bot.state.lock().unwrap();
                    (st.route.name.clone(), st.route.steps.len())
                };
                if dirty {
                    ui.label(
                        RichText::new("● sin guardar")
                            .font(prop(11.5))
                            .color(c.naranja),
                    );
                }
                if !name.is_empty() || mode == Mode::Recording {
                    ui.label(
                        RichText::new(format!("{name} · {n} pasos"))
                            .font(prop(11.5))
                            .color(c.suave),
                    );
                }
            });
        });
    }

    fn dialogo(&mut self, ctx: &egui::Context) {
        let Some(p) = &self.pendiente else { return };
        let c = pal();
        let name = self.bot.state.lock().unwrap().route.name.clone();
        let accion = match p {
            Pendiente::Cargar(r) => format!("abrir «{r}»"),
            Pendiente::Cerrar => "salir".to_string(),
        };
        let mut elegido = None;
        egui::Modal::new(egui::Id::unique("sin_guardar"))
            .frame(card())
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.horizontal(|ui| {
                    let (rid, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                    theme::dibujar_icono(
                        &ui.painter().clone(),
                        rid.center(),
                        9.0,
                        "aviso",
                        c.naranja,
                    );
                    ui.label(
                        RichText::new("Cambios sin guardar")
                            .font(titulo(17.0))
                            .color(c.texto),
                    );
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "La ruta «{name}» tiene cambios. ¿Qué hago antes de {accion}?"
                    ))
                    .font(prop(13.0))
                    .color(c.suave),
                );
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui.add(theme::boton_pri("Guardar", c.acento)).clicked() {
                        elegido = Some(1);
                    }
                    if ui.add(theme::boton_sec("Descartar cambios")).clicked() {
                        elegido = Some(2);
                    }
                    if ui.add(theme::boton_sec("Cancelar")).clicked() {
                        elegido = Some(0);
                    }
                });
            });
        let Some(e) = elegido else { return };
        let p = self.pendiente.take().unwrap();
        if e == 0 {
            return;
        }
        if e == 1 {
            self.guardar();
        }
        match p {
            Pendiente::Cargar(r) => self.cargar(&r),
            Pendiente::Cerrar => {
                self.resnap(); // ya guardado o descartado
                self.quit.store(true, Ordering::SeqCst);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

// ── Vistas ──────────────────────────────────────────────────────────────
impl App {
    fn vista_inicio(&mut self, ui: &mut Ui, mode: Mode) {
        let c = pal();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(14.0);
            ui.vertical_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::new(250.0, 52.0), Sense::hover());
                let p = ui.painter().clone();
                theme::marca(&p, Pos2::new(r.left() + 26.0, r.center().y), 17.0, c.acento);
                p.text(Pos2::new(r.left() + 62.0, r.center().y), Align2::LEFT_CENTER, "Replica", titulo(38.0), c.texto);
                ui.label(
                    RichText::new("Graba una vez lo que haces con ratón y teclado; Replica lo repite mirando la pantalla.")
                        .font(prop(13.5))
                        .color(c.suave),
                );
            });
            ui.add_space(22.0);

            let (n_rutas, steps, secs, activas, atajos) = {
                let st = self.bot.state.lock().unwrap();
                let atajos = ROUTE_HOTKEYS.iter().filter(|k| self.bot.hotkey_owner(**k).is_some()).count();
                (
                    self.routes.len(),
                    st.route.steps.len(),
                    st.route.duration_ms() as f64 / 1000.0,
                    st.config.schedules.iter().filter(|s| s.enabled).count(),
                    atajos,
                )
            };
            ui.columns(4, |cols| {
                theme::stat_card(&mut cols[0], "Rutas", &n_rutas.to_string(), "guardadas en esta carpeta", c.acento);
                theme::stat_card(&mut cols[1], "Ruta abierta", &steps.to_string(), &format!("pasos · {secs:.1} s"), c.verde);
                theme::stat_card(&mut cols[2], "Programadas", &activas.to_string(), "se lanzan solas", c.naranja);
                theme::stat_card(&mut cols[3], "Atajos", &atajos.to_string(), "de F1 a F8", c.suave);
            });
            ui.add_space(18.0);
            ui.label(RichText::new("¿Qué quieres hacer?").font(titulo(14.0)).color(c.texto));
            ui.add_space(6.0);
            ui.columns(3, |cols| {
                if theme::tarjeta_accion(&mut cols[0], "grabar", "Grabar una ruta", "Pulsa F9 en el juego, haz la tarea una vez y pulsa F9 otra vez.")
                    && mode == Mode::Idle
                {
                    self.vista = Vista::Rutas;
                }
                if theme::tarjeta_accion(&mut cols[1], "rutas", "Abrir y editar rutas", "Ajustes por ruta, paso a paso, deshacer y anclas visuales.") {
                    self.vista = Vista::Rutas;
                }
                if theme::tarjeta_accion(&mut cols[2], "reloj", "Programar", "Lanza una ruta cada día a una hora o cada X minutos.") {
                    self.vista = Vista::Programador;
                }
            });
            ui.add_space(18.0);
            theme::tarjeta(ui, "Atajos", |ui| {
                egui::Grid::new("atajos").num_columns(2).spacing([24.0, 6.0]).show(ui, |ui| {
                    for (k, t) in [
                        ("F9", "Empezar / terminar la grabación"),
                        ("F10", "Reproducir / parar la ruta abierta"),
                        ("F11", "Pausar / seguir (suelta las teclas mantenidas)"),
                        ("F12", "Corte de emergencia"),
                        ("F1–F8", "Lanzar la ruta que tenga ese atajo (Rutas → Ajustes de la ruta)"),
                        ("Ctrl+Z / Ctrl+S", "Deshacer / guardar en el editor"),
                    ] {
                        chip(ui, k, c.acento);
                        ui.label(RichText::new(t).font(prop(12.5)).color(c.suave));
                        ui.end_row();
                    }
                });
            });
        });
    }

    fn vista_rutas(
        &mut self,
        ui: &mut Ui,
        ctx: &egui::Context,
        mode: Mode,
        saving: bool,
        dirty: bool,
    ) {
        let editable = mode == Mode::Idle && !saving;
        egui::Panel::left("lista_rutas")
            .resizable(false)
            .exact_size(250.0)
            .frame(card())
            .show(ui, |ui| self.panel_lista(ui, editable));
        egui::Panel::right("ajustes_ruta")
            .resizable(false)
            .exact_size(300.0)
            .frame(card())
            .show(ui, |ui| {
                ui.add_enabled_ui(mode != Mode::Playing, |ui| self.panel_ajustes(ui));
            });
        egui::CentralPanel::default()
            .frame(card().outer_margin(Margin::symmetric(10, 0)))
            .show(ui, |ui| self.panel_editor(ui, ctx, editable, dirty));
    }

    fn panel_lista(&mut self, ui: &mut Ui, editable: bool) {
        let c = pal();
        theme::encabezado_panel(ui, "Rutas");
        ui.label(
            RichText::new("Nombre para la próxima grabación")
                .font(prop(11.5))
                .color(c.suave),
        );
        ui.add(
            egui::TextEdit::singleline(&mut self.bot.state.lock().unwrap().record_name)
                .desired_width(f32::INFINITY),
        );
        ui.add_space(8.0);
        if self.routes.is_empty() {
            theme::ayuda(
                ui,
                "Aún no hay rutas. Pulsa F9 en el juego para grabar la primera.",
            );
        }
        let actual = self.bot.state.lock().unwrap().route.name.clone();
        let mut abrir = None;
        egui::ScrollArea::vertical()
            .id_salt("rlist")
            .auto_shrink([false, false])
            .max_height(ui.available_height() - 40.0)
            .show(ui, |ui| {
                for name in &self.routes {
                    let sel = *name == actual;
                    let id = ui.make_persistent_id(("ruta", name));
                    let hov = ui
                        .ctx()
                        .read_response(id)
                        .map(|r| r.hovered())
                        .unwrap_or(false);
                    let (rect, _) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());
                    let p = ui.painter();
                    if sel {
                        p.rect_filled(rect, CornerRadius::same(7), c.acento.gamma_multiply(0.2));
                    } else if hov {
                        p.rect_filled(rect, CornerRadius::same(7), c.borde);
                    }
                    theme::dibujar_icono(
                        p,
                        Pos2::new(rect.left() + 14.0, rect.center().y),
                        6.5,
                        "rutas",
                        if sel { c.acento } else { c.suave },
                    );
                    p.text(
                        Pos2::new(rect.left() + 28.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        name,
                        prop(13.0),
                        if sel { c.texto } else { c.suave },
                    );
                    if let Some(k) = ROUTE_HOTKEYS
                        .iter()
                        .find(|k| self.bot.hotkey_owner(**k).as_deref() == Some(name.as_str()))
                    {
                        p.text(
                            rect.right_center() - Vec2::new(8.0, 0.0),
                            Align2::RIGHT_CENTER,
                            format!("{k:?}"),
                            mono(11.0),
                            c.acento,
                        );
                    }
                    if ui.interact(rect, id, Sense::click()).clicked() && editable && !sel {
                        abrir = Some(name.clone());
                    }
                }
            });
        if let Some(n) = abrir {
            self.pedir_cargar(n);
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            if ui.add(theme::boton_mini("Actualizar lista")).clicked() {
                self.routes = Route::list();
                self.bot.refresh_hotkeys();
            }
        });
    }

    fn panel_ajustes(&mut self, ui: &mut Ui) {
        let c = pal();
        theme::encabezado_panel(ui, "Ajustes de la ruta");
        theme::ayuda(ui, "Se guardan con la ruta: cada ruta recuerda los suyos.");
        ui.add_space(6.0);
        let bot = self.bot.clone();
        egui::ScrollArea::vertical().id_salt("ajustes").auto_shrink([false, false]).show(ui, |ui| {
            let mut st = bot.state.lock().unwrap();
            let route_name = st.route.name.clone();
            let s = &mut st.settings;
            ui.label(RichText::new("REPRODUCCIÓN").font(mono(10.0)).color(c.suave));
            ui.add(egui::Slider::new(&mut s.speed, 0.25..=4.0).text("velocidad ×"));
            ui.add(egui::DragValue::new(&mut s.loops).prefix("vueltas ").suffix("  (0 = ∞)"));
            ui.add(egui::DragValue::new(&mut s.pause_between_loops_ms).prefix("pausa entre vueltas ").suffix(" ms").speed(10));
            ui.add(egui::Slider::new(&mut s.countdown_s, 0..=10).text("cuenta atrás s"));
            ui.checkbox(&mut s.humanize, "Humanizar tiempos y posición");
            ui.checkbox(&mut s.record_moves, "Grabar movimientos del ratón");
            ui.add_space(10.0);

            ui.label(RichText::new("ATAJO DE LA RUTA").font(mono(10.0)).color(c.suave));
            let actual = s.hotkey.map(|k| format!("{k:?}")).unwrap_or_else(|| "ninguno".into());
            egui::ComboBox::from_id_salt("hotkey").selected_text(actual).width(140.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut s.hotkey, None, "ninguno");
                for k in ROUTE_HOTKEYS {
                    let owner = bot.hotkey_owner(k).filter(|o| *o != route_name);
                    let txt = match owner {
                        Some(o) => format!("{k:?}  (ahora: {o})"),
                        None => format!("{k:?}"),
                    };
                    ui.selectable_value(&mut s.hotkey, Some(k), txt);
                }
            });
            theme::ayuda(ui, "Pulsa esa tecla en cualquier momento (con Replica en espera) para lanzar esta ruta. Se activa al guardar.");
            ui.add_space(10.0);

            ui.label(RichText::new("VISIÓN").font(mono(10.0)).color(c.suave));
            ui.checkbox(&mut s.use_anchors, "Verificar la pantalla antes de cada clic");
            ui.add(egui::Slider::new(&mut s.threshold, 0.5..=1.0).text("similitud"));
            ui.add(egui::Slider::new(&mut s.search_radius, 0..=400).text("radio px"));
            ui.add(egui::DragValue::new(&mut s.anchor_timeout_ms).prefix("esperar ancla ").suffix(" ms").speed(10));
            ui.label(RichText::new("Si no se puede verificar un clic:").font(prop(12.0)).color(c.suave));
            theme::pildoras(ui, &mut s.on_missing, &[(MissingAnchor::ClickAnyway, "clicar igual"), (MissingAnchor::Stop, "detener")]);
        });
    }

    fn panel_editor(&mut self, ui: &mut Ui, ctx: &egui::Context, editable: bool, dirty: bool) {
        let c = pal();
        let bot = self.bot.clone();
        let (name, n, secs, target, cursor) = {
            let st = bot.state.lock().unwrap();
            (
                st.route.name.clone(),
                st.route.steps.len(),
                st.route.duration_ms() as f64 / 1000.0,
                st.route.target.clone(),
                st.step_cursor,
            )
        };
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(if name.is_empty() {
                    "Sin ruta abierta".into()
                } else {
                    name.clone()
                })
                .font(titulo(18.0))
                .color(c.texto),
            );
            ui.label(
                RichText::new(format!("{n} pasos · {secs:.1} s"))
                    .font(mono(12.0))
                    .color(c.suave),
            );
            if dirty {
                chip(ui, "sin guardar", c.naranja);
            }
        });
        ui.add_space(6.0);

        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(theme::boton_pri("Guardar", c.acento))
                    .on_hover_text("Ctrl+S")
                    .clicked()
                {
                    self.guardar();
                }
                if ui
                    .add_enabled(!self.undo.is_empty(), theme::boton_mini("Deshacer"))
                    .on_hover_text("Ctrl+Z")
                    .clicked()
                {
                    self.deshacer();
                }
                if ui
                    .add_enabled(n > 0, theme::boton_mini("Ejecutar paso"))
                    .on_hover_text(format!(
                        "Ejecuta solo el paso {} y avanza al siguiente (para depurar)",
                        cursor + 1
                    ))
                    .clicked()
                {
                    bot.step_once();
                }
                if ui
                    .add(theme::boton_mini("Simplificar movimientos"))
                    .on_hover_text("Quita los movimientos de ratón intermedios")
                    .clicked()
                {
                    let before = n;
                    self.edit(|r| simplify_moves(&mut r.steps));
                    let after = bot.state.lock().unwrap().route.steps.len();
                    bot.log(format!("Simplificada: {before} → {after} pasos."));
                }
            });
            ui.add_space(4.0);
            // Ventana objetivo.
            ui.horizontal(|ui| {
                ui.label(RichText::new("Ventana").font(prop(12.0)).color(c.suave));
                let shown = target.as_ref().map_or(
                    "pantalla completa (coordenadas absolutas)".into(),
                    Target::label,
                );
                let mut pick: Option<Option<Target>> = None;
                egui::ComboBox::from_id_salt("target")
                    .selected_text(shown)
                    .width(360.0)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(false, "actualizar lista").clicked()
                            || self.windows.is_empty()
                        {
                            self.windows = crate::vision::list_windows();
                        }
                        if ui
                            .selectable_label(target.is_none(), "pantalla completa")
                            .clicked()
                        {
                            pick = Some(None);
                        }
                        for w in &self.windows {
                            if ui
                                .selectable_label(target.as_ref() == Some(w), w.label())
                                .clicked()
                            {
                                pick = Some(Some(w.clone()));
                            }
                        }
                    });
                if let Some(t) = pick {
                    self.edit(|r| r.target = t);
                }
            });
            // Añadir pasos.
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_text)
                        .desired_width(150.0)
                        .hint_text("texto (ñ, tildes…)"),
                );
                if ui.add(theme::boton_mini("+ Escribir texto")).clicked() {
                    let t = self.new_text.clone();
                    self.edit(|r| push(r, Action::TypeText { text: t }));
                }
                if ui.add(theme::boton_mini("+ Enter")).clicked() {
                    self.edit(|r| {
                        push(
                            r,
                            Action::KeyDown {
                                key: rdev::Key::Return,
                            },
                        );
                        push(
                            r,
                            Action::KeyUp {
                                key: rdev::Key::Return,
                            },
                        );
                    });
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_cmd)
                        .desired_width(150.0)
                        .hint_text("notepad.exe / steam://…"),
                );
                if ui.add(theme::boton_mini("+ Ejecutar programa")).clicked()
                    && !self.new_cmd.trim().is_empty()
                {
                    let cmd = self.new_cmd.clone();
                    self.edit(|r| push(r, Action::RunCommand { command: cmd }));
                    // Lo ha escrito el propio usuario: queda permitido.
                    let mut st = bot.state.lock().unwrap();
                    let r = st.route.clone();
                    st.config.trust(&r);
                    drop(st);
                    bot.save_config();
                }
                ui.add(
                    egui::DragValue::new(&mut self.new_wait)
                        .suffix(" ms")
                        .speed(10),
                );
                if ui.add(theme::boton_mini("+ Esperar")).clicked() {
                    let ms = self.new_wait;
                    self.edit(|r| push(r, Action::Wait { ms }));
                }
            });
        });

        // Programas sin permitir.
        let (commands, ok) = {
            let st = bot.state.lock().unwrap();
            (st.route.commands(), st.commands_ok())
        };
        if !ok {
            ui.add_space(6.0);
            egui::Frame::new()
                .stroke(Stroke::new(1.0, c.rojo))
                .fill(c.rojo.gamma_multiply(0.08))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.label(RichText::new("⚠ Esta ruta ejecuta programas en tu equipo:").font(titulo(13.0)).color(c.rojo));
                    for cmd in &commands {
                        ui.label(RichText::new(format!("   {cmd}")).font(mono(12.0)).color(c.texto));
                    }
                    ui.set_width(ui.available_width());
                    theme::ayuda(ui, "Permítelo solo si confías en quien te pasó la ruta. Se recuerda hasta que cambien los comandos.");
                    ui.add_space(4.0);
                    if ui.add(theme::boton_pri("Permitir", c.rojo)).clicked() {
                        let mut st = bot.state.lock().unwrap();
                        let r = st.route.clone();
                        st.config.trust(&r);
                        drop(st);
                        bot.save_config();
                        bot.log("Programas de la ruta permitidos.");
                    }
                });
        }
        ui.add_space(8.0);
        theme::encabezado_panel(ui, "Secuencia");

        if n == 0 {
            theme::panel_vacio(
                ui,
                "grabar",
                "Sin pasos",
                "Pulsa F9 en el juego, haz la tarea y pulsa F9 otra vez.",
            );
            return;
        }
        let row_h = 30.0;
        let mut ediciones: Vec<Edicion> = vec![];
        let mut nuevo_cursor = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_h, n, |ui, range| {
                let steps: Vec<Step> = {
                    let st = bot.state.lock().unwrap();
                    let end = range.end.min(st.route.steps.len());
                    st.route.steps[range.start.min(end)..end].to_vec()
                };
                for (i, step) in range.zip(steps.iter()) {
                    let (tag, col) = tag(&step.action);
                    let es_cursor = i == cursor;
                    let fill = if es_cursor {
                        c.acento.gamma_multiply(0.15)
                    } else if i % 2 == 0 {
                        c.borde.gamma_multiply(0.35)
                    } else {
                        Color32::TRANSPARENT
                    };
                    egui::Frame::new()
                        .fill(fill)
                        .corner_radius(CornerRadius::same(5))
                        .inner_margin(Margin::symmetric(6, 2))
                        .show(ui, |ui| {
                            ui.set_height(row_h - 4.0);
                            ui.horizontal_centered(|ui| {
                                let (r, resp) =
                                    ui.allocate_exact_size(Vec2::new(40.0, 20.0), Sense::click());
                                ui.painter().text(
                                    r.left_center(),
                                    Align2::LEFT_CENTER,
                                    format!("{:03}", i + 1),
                                    mono(11.5),
                                    if es_cursor { c.acento } else { c.medio },
                                );
                                if resp
                                    .on_hover_text("Marcar como siguiente paso de «Ejecutar paso»")
                                    .clicked()
                                {
                                    nuevo_cursor = Some(i);
                                }
                                let (r, _) =
                                    ui.allocate_exact_size(Vec2::new(46.0, 20.0), Sense::hover());
                                ui.painter().text(
                                    r.left_center(),
                                    Align2::LEFT_CENTER,
                                    tag,
                                    titulo(11.5),
                                    col,
                                );
                                ui.add_enabled_ui(editable, |ui| {
                                    let mut s = step.clone();
                                    let mut edited = ui
                                        .add(
                                            egui::DragValue::new(&mut s.delay_ms)
                                                .suffix(" ms")
                                                .speed(5),
                                        )
                                        .changed();
                                    match &mut s.action {
                                        Action::TypeText { text } => {
                                            edited |= ui
                                                .add(
                                                    egui::TextEdit::singleline(text)
                                                        .desired_width(220.0),
                                                )
                                                .changed();
                                        }
                                        Action::RunCommand { command } => {
                                            edited |= ui
                                                .add(
                                                    egui::TextEdit::singleline(command)
                                                        .desired_width(220.0),
                                                )
                                                .changed();
                                        }
                                        Action::Wait { ms } => {
                                            edited |= ui
                                                .add(
                                                    egui::DragValue::new(ms)
                                                        .prefix("esperar ")
                                                        .suffix(" ms")
                                                        .speed(10),
                                                )
                                                .changed();
                                        }
                                        Action::ButtonDown {
                                            anchor: Some(file), ..
                                        } => {
                                            let path = Route::dir(&name).join(file.as_str());
                                            let r = ui.label(
                                                RichText::new(step.action.describe())
                                                    .font(prop(12.5))
                                                    .color(c.texto),
                                            );
                                            if let Some(tex) = self.anchor_texture(ctx, path) {
                                                r.on_hover_ui(|ui| {
                                                    ui.label(
                                                        RichText::new(
                                                            "Lo que Replica buscará en pantalla",
                                                        )
                                                        .font(prop(11.5))
                                                        .color(c.suave),
                                                    );
                                                    ui.image((tex.id(), Vec2::splat(144.0)));
                                                });
                                            }
                                        }
                                        other => {
                                            let col = if matches!(other, Action::MouseMove { .. }) {
                                                c.medio
                                            } else {
                                                c.texto
                                            };
                                            ui.label(
                                                RichText::new(other.describe())
                                                    .font(prop(12.5))
                                                    .color(col),
                                            );
                                        }
                                    }
                                    if edited {
                                        ediciones.push(Edicion::Cambiar(i, s));
                                    }
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if ui
                                            .add(theme::boton_mini("×"))
                                            .on_hover_text("Borrar paso")
                                            .clicked()
                                        {
                                            ediciones.push(Edicion::Borrar(i));
                                        }
                                        if i > 0
                                            && ui
                                                .add(theme::boton_mini("↑"))
                                                .on_hover_text("Subir paso")
                                                .clicked()
                                        {
                                            ediciones.push(Edicion::Subir(i));
                                        }
                                    });
                                });
                            });
                        });
                }
            });
        if let Some(i) = nuevo_cursor {
            bot.state.lock().unwrap().step_cursor = i;
        }
        for e in ediciones {
            // Cambios seguidos en la misma fila (arrastrar un retardo, escribir) = un solo «deshacer».
            let fila = if let Edicion::Cambiar(i, _) = &e {
                Some(*i)
            } else {
                None
            };
            let agrupar = fila.is_some() && fila == self.ultima_fila && !self.undo.is_empty();
            self.ultima_fila = fila;
            let apply = move |r: &mut Route| match e {
                Edicion::Cambiar(i, s) => r.steps[i] = s,
                Edicion::Borrar(i) => {
                    r.steps.remove(i);
                }
                Edicion::Subir(i) => r.steps.swap(i, i - 1),
            };
            if agrupar {
                apply(&mut bot.state.lock().unwrap().route);
            } else {
                self.edit(apply);
            }
        }
    }

    fn vista_programador(&mut self, ui: &mut Ui) {
        let c = pal();
        let bot = self.bot.clone();
        theme::titulo_vista(
            ui,
            "reloj",
            "Programador",
            "Lanza rutas solas a una hora o cada X minutos. Replica tiene que estar abierta (o en la bandeja).",
        );
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            theme::tarjeta(ui, "Nueva programación", |ui| {
                if self.f_route.is_empty() {
                    self.f_route = self.routes.first().cloned().unwrap_or_default();
                }
                ui.label(RichText::new("Ruta").font(prop(12.0)).color(c.suave));
                egui::ComboBox::from_id_salt("f_route").selected_text(self.f_route.clone()).width(240.0).show_ui(ui, |ui| {
                    for r in &self.routes {
                        ui.selectable_value(&mut self.f_route, r.clone(), r);
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Cuándo").font(prop(12.0)).color(c.suave));
                theme::pildoras(ui, &mut self.f_daily, &[(true, "Cada día a una hora"), (false, "Cada X minutos")]);
                ui.horizontal(|ui| {
                    if self.f_daily {
                        ui.add(egui::DragValue::new(&mut self.f_hour).range(0..=23).prefix("hora "));
                        ui.add(egui::DragValue::new(&mut self.f_min).range(0..=59).prefix("min "));
                    } else {
                        ui.add(egui::DragValue::new(&mut self.f_every).range(1..=1440).prefix("cada ").suffix(" min"));
                    }
                });
                ui.add_space(10.0);
                if ui.add_enabled(!self.f_route.is_empty(), theme::boton_pri("Añadir", c.acento)).clicked() {
                    let when = if self.f_daily { When::Daily { hour: self.f_hour, minute: self.f_min } } else { When::Every { minutes: self.f_every } };
                    let mut s = Schedule { route: self.f_route.clone(), when, enabled: true, last: None };
                    s.arm(chrono::Local::now());
                    bot.log(format!("Programada '{}' {}.", s.route, s.label()));
                    bot.state.lock().unwrap().config.schedules.push(s);
                    bot.save_config();
                }
                ui.add_space(6.0);
                theme::ayuda(ui, "Si a esa hora Replica está ocupada (grabando o reproduciendo), esa vez se salta. Las rutas con programas solo se lanzan si ya los permitiste.");
            });
            let ui = &mut cols[1];
            theme::tarjeta(ui, "Programaciones", |ui| {
                let mut st = bot.state.lock().unwrap();
                if st.config.schedules.is_empty() {
                    theme::ayuda(ui, "Ninguna todavía.");
                }
                let mut borrar = None;
                let mut cambio = false;
                for (i, s) in st.config.schedules.iter_mut().enumerate() {
                    egui::Frame::new().fill(c.borde.gamma_multiply(0.4)).corner_radius(CornerRadius::same(7)).inner_margin(Margin::symmetric(10, 6)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut s.enabled, "").changed() {
                                s.last = None;
                                s.arm(chrono::Local::now());
                                cambio = true;
                            }
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&s.route).font(titulo(13.5)).color(if s.enabled { c.texto } else { c.medio }));
                                let ult = s.last.map(|l| format!(" · última {}", l.format("%d/%m %H:%M"))).unwrap_or_default();
                                ui.label(RichText::new(format!("{}{ult}", s.label())).font(prop(11.5)).color(c.suave));
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.add(theme::boton_mini("×")).on_hover_text("Quitar").clicked() {
                                    borrar = Some(i);
                                }
                            });
                        });
                    });
                    ui.add_space(4.0);
                }
                if let Some(i) = borrar {
                    st.config.schedules.remove(i);
                    cambio = true;
                }
                drop(st);
                if cambio {
                    bot.save_config();
                }
            });
        });
    }

    fn vista_registro(&mut self, ui: &mut Ui) {
        let c = pal();
        theme::titulo_vista(
            ui,
            "registro",
            "Registro",
            "Todo lo que ha hecho Replica en esta sesión.",
        );
        let log = self.bot.state.lock().unwrap().log.clone();
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show_rows(ui, 18.0, log.len(), |ui, range| {
                    for line in log.range(range) {
                        let low = line.to_lowercase();
                        let col = if low.contains("error")
                            || low.contains("no se pudo")
                            || line.starts_with('×')
                        {
                            c.rojo
                        } else if line.starts_with('✔') || line.starts_with('▶') {
                            c.verde
                        } else if line.starts_with('●')
                            || line.starts_with('■')
                            || line.starts_with("En pausa")
                            || low.contains("ancla")
                        {
                            c.naranja
                        } else {
                            c.suave
                        };
                        ui.add(
                            egui::Label::new(RichText::new(line).font(mono(12.0)).color(col))
                                .truncate(),
                        );
                    }
                });
        });
    }

    fn vista_ajustes(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let c = pal();
        let bot = self.bot.clone();
        theme::titulo_vista(
            ui,
            "engranaje",
            "Ajustes",
            "Apariencia y comportamiento de la ventana.",
        );
        ui.set_max_width(680.0);
        theme::tarjeta(ui, "Apariencia", |ui| {
            ui.label(RichText::new("Máscara").font(prop(12.0)).color(c.suave));
            let mut t = theme::tema();
            let ops: Vec<(u8, &str)> = theme::MASCARAS
                .iter()
                .enumerate()
                .map(|(i, (n, _))| (i as u8, *n))
                .collect();
            if theme::pildoras(ui, &mut t, &ops) {
                theme::set_tema(ctx, t);
                bot.state.lock().unwrap().config.tema = t;
                bot.save_config();
            }
        });
        ui.add_space(10.0);
        theme::tarjeta(ui, "Bandeja del sistema", |ui| {
            if cfg!(windows) {
                let mut tray = !bot.state.lock().unwrap().config.no_tray;
                if ui
                    .checkbox(&mut tray, "Al cerrar la ventana, seguir en la bandeja")
                    .changed()
                {
                    bot.state.lock().unwrap().config.no_tray = !tray;
                    bot.save_config();
                }
                theme::ayuda(
                    ui,
                    "Así los atajos F1–F8 y el programador siguen funcionando. Para salir del todo: clic derecho en el icono → Salir.",
                );
            } else {
                theme::ayuda(
                    ui,
                    "La bandeja del sistema solo está disponible en Windows.",
                );
            }
        });
        ui.add_space(10.0);
        theme::tarjeta(ui, "Acerca de", |ui| {
            ui.label(
                RichText::new(format!("Replica v{}", env!("CARGO_PKG_VERSION")))
                    .font(prop(13.0))
                    .color(c.texto),
            );
            ui.hyperlink_to(
                "github.com/AngelazoGit/Replica",
                "https://github.com/AngelazoGit/Replica",
            );
            theme::ayuda(
                ui,
                "Los datos (rutas y replica.json) se guardan junto al ejecutable: es portable.",
            );
        });
    }
}

/// Etiqueta corta y color para cada tipo de paso.
fn tag(a: &Action) -> (&'static str, Color32) {
    let c = pal();
    match a {
        Action::MouseMove { .. } => ("MOV", c.medio),
        Action::ButtonDown { .. } | Action::ButtonUp { .. } => ("CLIC", c.acento),
        Action::Wheel { .. } => ("RUEDA", c.acento),
        Action::KeyDown { .. } | Action::KeyUp { .. } => ("TECLA", c.naranja),
        Action::TypeText { .. } => ("TEXTO", c.verde),
        Action::RunCommand { .. } => ("EXEC", c.rojo),
        Action::Wait { .. } => ("ESPERA", c.suave),
    }
}

fn push(r: &mut Route, action: Action) {
    r.steps.push(Step {
        delay_ms: 300,
        action,
    });
}
