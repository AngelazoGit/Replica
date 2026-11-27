//! Núcleo del bot: escucha global de ratón/teclado (grabación + atajos),
//! y reproductor de rutas con verificación visual.

use crate::config::Config;
use crate::i18n::{tr, trf};
use crate::model::{Action, MissingAnchor, Route, Settings, Step, Target, UNTIL_FILE, Until};
use crate::vision;
use image::RgbaImage;
use rdev::{Button, EventType, Key};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

pub const HOTKEY_RECORD: Key = Key::F9;
pub const HOTKEY_PLAY: Key = Key::F10;
pub const HOTKEY_PAUSE: Key = Key::F11;
pub const HOTKEY_STOP: Key = Key::F12;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    Idle,
    Recording,
    Playing,
}

pub struct State {
    pub mode: Mode,
    /// Ruta cargada/en edición (la que se reproduce).
    pub route: Route,
    pub settings: Settings,
    pub record_name: String,
    pub log: VecDeque<String>,
    pub progress: String,
    pub play: Progress,
    /// Configuración de la app (máscara, programaciones, rutas permitidas).
    pub config: Config,
    /// Siguiente paso de «ejecutar un paso».
    pub step_cursor: usize,
    /// Aviso pendiente para la interfaz al terminar una reproducción: (bien, mensaje).
    pub aviso: Option<(bool, String)>,
    /// La interfaz tiene cambios sin guardar en la ruta abierta (un atajo o el
    /// programador no deben cargar otra ruta encima).
    pub unsaved: bool,
    mouse: (f64, f64),
    last_step_at: Instant,
    last_move_at: Instant,
    /// Atajo mantenido pulsado: ignora la autorrepetición del SO.
    hotkey_held: Option<Key>,
    /// Ruta cargada antes de grabar: se recupera si la grabación sale vacía.
    prev_route: Route,
    /// Identificador de la grabación en curso: los trabajos de anclas de una grabación
    /// anterior no pueden tocar la actual.
    rec_id: u64,
    /// Ventana bajo el primer clic de la grabación `rec_id`.
    rec_target: Option<Target>,
    /// Anclas de la grabación `rec_id` que no se pudieron guardar (se quitan de la ruta).
    rec_bad_anchors: Vec<String>,
}

impl State {
    /// La ruta cargada no ejecuta programas o el usuario ya los permitió.
    pub fn commands_ok(&self) -> bool {
        self.config.is_trusted(&self.route)
    }
}

/// Teclas que se pueden asignar como atajo de una ruta.
pub const ROUTE_HOTKEYS: [Key; 8] = [
    Key::F1,
    Key::F2,
    Key::F3,
    Key::F4,
    Key::F5,
    Key::F6,
    Key::F7,
    Key::F8,
];

/// Progreso de la reproducción, para la cabecera y la barra de progreso.
#[derive(Debug, Clone, Default)]
pub struct Progress {
    pub lap: u32,
    /// 0 = infinitas.
    pub laps: u32,
    pub step: usize,
    pub steps: usize,
    /// Segundos que faltan de la cuenta atrás (0 = no hay cuenta atrás).
    pub countdown: u32,
}

impl Progress {
    /// Fracción de la vuelta actual (0..1).
    pub fn frac(&self) -> f32 {
        if self.steps == 0 {
            0.0
        } else {
            self.step as f32 / self.steps as f32
        }
    }
}

/// Fotograma reciente alrededor del cursor: (imagen, x0, y0) en coordenadas globales.
type Frame = Arc<(RgbaImage, i32, i32)>;

/// Petición de guardar un ancla: grabación, punto del clic, PNG destino y fotograma previo al clic.
type AnchorJob = (u64, f64, f64, std::path::PathBuf, Option<Frame>);

/// Lado del fotograma que se captura continuamente alrededor del cursor al grabar.
const FRAME_SIZE: u32 = 256;

pub struct Bot {
    pub state: Mutex<State>,
    stop: AtomicBool,
    repaint: Mutex<Option<eframe::egui::Context>>,
    anchor_tx: Mutex<mpsc::Sender<AnchorJob>>,
    frame: Mutex<Option<Frame>>,
    /// Anclas en cola o procesándose.
    pending: AtomicUsize,
    /// Guardando una grabación (esperando sus anclas): no se puede reproducir ni grabar.
    saving: AtomicBool,
    paused: AtomicBool,
    /// Atajo de ruta (F1–F8) → nombre de la ruta.
    route_hotkeys: Mutex<HashMap<Key, String>>,
}

impl Bot {
    pub fn new() -> Arc<Bot> {
        let (tx, rx) = mpsc::channel::<AnchorJob>();
        let bot = Arc::new(Bot {
            state: Mutex::new(State {
                mode: Mode::Idle,
                // Sin nombre: si no, aparecería marcada en la lista una ruta guardada que no está cargada.
                route: Route::default(),
                settings: Settings::default(),
                record_name: String::new(), // vacío = nombre automático con fecha y hora
                log: VecDeque::new(),
                progress: String::new(),
                play: Progress::default(),
                config: Config::load(),
                step_cursor: 0,
                unsaved: false,
                aviso: None,
                mouse: (0.0, 0.0),
                last_step_at: Instant::now(),
                last_move_at: Instant::now(),
                hotkey_held: None,
                prev_route: Route::default(),
                rec_id: 0,
                rec_target: None,
                rec_bad_anchors: vec![],
            }),
            stop: AtomicBool::new(false),
            repaint: Mutex::new(None),
            anchor_tx: Mutex::new(tx),
            frame: Mutex::new(None),
            pending: AtomicUsize::new(0),
            saving: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            route_hotkeys: Mutex::new(HashMap::new()),
        });
        bot.refresh_hotkeys();

        // Programador: cada segundo mira si toca lanzar alguna ruta.
        let b = bot.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(1));
                b.tick_schedules(chrono::Local::now());
            }
        });

        // Hilo que guarda anclas visuales sin bloquear la escucha de entrada.
        let b = bot.clone();
        thread::spawn(move || {
            for (rec_id, x, y, path, frame) in rx {
                // Grabación vacía ya descartada: su carpeta se borró.
                if path.parent().is_some_and(|d| d.exists()) {
                    // Preferimos el fotograma de antes del clic; si no contiene el punto, captura en vivo.
                    let img = match frame.and_then(|f| vision::anchor_from_frame(&f, x, y)) {
                        Some(img) => Ok(img),
                        None => vision::capture_anchor(x, y),
                    };
                    if let Err(e) = img.and_then(|img| Ok(img.save(&path)?)) {
                        b.log(trf("Clic sin ancla visual: {}", &[&e]));
                        let mut st = b.state.lock().unwrap();
                        if st.rec_id == rec_id {
                            let file = path.file_name().unwrap().to_string_lossy().into_owned();
                            st.rec_bad_anchors.push(file);
                        }
                    }
                    // Ventana objetivo = la que está bajo el primer clic, detectada al momento
                    // (al parar la grabación podría haberse movido o cerrado).
                    let need = {
                        let st = b.state.lock().unwrap();
                        st.rec_id == rec_id && st.rec_target.is_none()
                    };
                    if need {
                        let target = vision::window_at(x, y);
                        let mut st = b.state.lock().unwrap();
                        if st.rec_id == rec_id && st.rec_target.is_none() {
                            st.rec_target = target;
                        }
                    }
                }
                b.pending.fetch_sub(1, Ordering::SeqCst);
            }
        });

        // Mientras se graba, captura continuamente la zona del cursor para que el ancla
        // muestre la pantalla *antes* del clic (el juego puede cambiarla al pulsar).
        let b = bot.clone();
        thread::spawn(move || {
            loop {
                let (mode, (x, y)) = {
                    let st = b.state.lock().unwrap();
                    (st.mode, st.mouse)
                };
                let frame = if mode == Mode::Recording {
                    let half = (FRAME_SIZE / 2) as i32;
                    vision::capture_region(x as i32 - half, y as i32 - half, FRAME_SIZE, FRAME_SIZE)
                        .ok()
                        .map(Arc::new)
                } else {
                    None
                };
                *b.frame.lock().unwrap() = frame;
                thread::sleep(Duration::from_millis(if mode == Mode::Recording {
                    40
                } else {
                    200
                }));
            }
        });

        // Hilo de escucha global (rdev::listen bloquea para siempre).
        let b = bot.clone();
        thread::spawn(move || {
            if let Err(e) = rdev::listen(move |ev| b.on_input(ev.event_type)) {
                eprintln!("Error escuchando entrada global: {e:?}");
            }
        });
        bot
    }

    pub fn set_repaint(&self, ctx: eframe::egui::Context) {
        *self.repaint.lock().unwrap() = Some(ctx);
    }

    /// Minimiza o restaura la ventana desde el motor: minimizada, la interfaz no se ejecuta
    /// y no podría restaurarse sola. No llamar con el estado bloqueado.
    fn minimizar(&self, si: bool) {
        if self.state.lock().unwrap().config.no_minimize {
            return;
        }
        if let Some(ctx) = self.repaint.lock().unwrap().as_ref() {
            ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Minimized(si));
            ctx.request_repaint();
        }
    }

    fn repaint(&self) {
        if let Some(ctx) = self.repaint.lock().unwrap().as_ref() {
            ctx.request_repaint();
        }
    }

    pub fn log(&self, msg: impl Into<String>) {
        let msg = msg.into();
        println!("{msg}");
        let mut st = self.state.lock().unwrap();
        st.log.push_back(msg);
        while st.log.len() > 200 {
            st.log.pop_front();
        }
        drop(st);
        self.repaint();
    }

    pub fn mode(&self) -> Mode {
        self.state.lock().unwrap().mode
    }

    // ───────────────────────── Entrada global ─────────────────────────

    fn on_input(self: &Arc<Self>, ev: EventType) {
        // Atajos globales: funcionan aunque el juego tenga el foco.
        match ev {
            EventType::KeyPress(k @ (HOTKEY_RECORD | HOTKEY_PLAY | HOTKEY_PAUSE | HOTKEY_STOP)) => {
                if self.state.lock().unwrap().hotkey_held.replace(k) == Some(k) {
                    return; // autorrepetición: si no, mantener F9 alterna grabar/parar sin control
                }
                return match k {
                    HOTKEY_RECORD => self.toggle_record(false),
                    HOTKEY_PLAY => self.toggle_play(),
                    HOTKEY_PAUSE => self.toggle_pause(),
                    _ => self.stop_all(),
                };
            }
            EventType::KeyRelease(HOTKEY_RECORD | HOTKEY_PLAY | HOTKEY_PAUSE | HOTKEY_STOP) => {
                self.state.lock().unwrap().hotkey_held = None;
                return;
            }
            // Atajo de una ruta: solo en reposo (grabando, F1–F8 se graban como teclas normales).
            EventType::KeyPress(k) if self.mode() == Mode::Idle => {
                let name = self.route_hotkeys.lock().unwrap().get(&k).cloned();
                if let Some(name) = name {
                    let bot = self.clone();
                    // En otro hilo: cargar la ruta no debe bloquear el hook de teclado.
                    thread::spawn(move || bot.play_named(&name, tr("atajo")));
                    return;
                }
            }
            _ => {}
        }

        let mut st = self.state.lock().unwrap();
        if let EventType::MouseMove { x, y } = ev {
            st.mouse = (x, y);
        }
        if st.mode != Mode::Recording {
            return; // en reproducción ignoramos nuestros propios eventos simulados
        }

        let now = Instant::now();
        let action = match ev {
            EventType::MouseMove { x, y } => {
                if !st.settings.record_moves
                    || now.duration_since(st.last_move_at) < Duration::from_millis(15)
                {
                    return;
                }
                st.last_move_at = now;
                Action::MouseMove { x, y }
            }
            EventType::ButtonPress(button) => {
                let (x, y) = st.mouse;
                let file = format!("anchor_{:04}.png", st.route.steps.len());
                let path = Route::dir(&st.route.name).join(&file);
                let frame = self.frame.lock().unwrap().clone();
                self.pending.fetch_add(1, Ordering::SeqCst);
                if self
                    .anchor_tx
                    .lock()
                    .unwrap()
                    .send((st.rec_id, x, y, path, frame))
                    .is_err()
                {
                    self.pending.fetch_sub(1, Ordering::SeqCst);
                }
                Action::ButtonDown {
                    button,
                    x,
                    y,
                    anchor: Some(file),
                }
            }
            EventType::ButtonRelease(button) => Action::ButtonUp { button },
            EventType::Wheel { delta_x, delta_y } => Action::Wheel {
                dx: delta_x,
                dy: delta_y,
            },
            EventType::KeyPress(key) => Action::KeyDown { key },
            EventType::KeyRelease(key) => Action::KeyUp { key },
        };
        let delay = if st.route.steps.is_empty() {
            0
        } else {
            now.duration_since(st.last_step_at).as_millis() as u64
        };
        st.last_step_at = now;
        st.route.steps.push(Step {
            delay_ms: delay,
            action,
        });
        let n = st.route.steps.len();
        st.progress = trf("Grabando… {} pasos", &[&n]);
        drop(st);
        self.repaint();
    }

    // ───────────────────────── Grabación ─────────────────────────

    pub fn toggle_record(self: &Arc<Self>, from_gui: bool) {
        let mut st = self.state.lock().unwrap();
        match st.mode {
            Mode::Playing => {
                drop(st);
                self.log(tr("Para la reproducción antes de grabar (F12)."));
            }
            Mode::Idle if self.saving.load(Ordering::SeqCst) => {
                drop(st);
                self.log(tr("Terminando de guardar la grabación anterior…"));
            }
            Mode::Idle if st.unsaved => {
                // Grabar sustituye la ruta abierta: sus cambios sin guardar se perderían.
                drop(st);
                self.log(tr(
                    "Guarda o descarta los cambios de la ruta abierta antes de grabar.",
                ));
            }
            Mode::Idle => {
                // Nunca pisar una ruta existente: nombre, nombre_2, nombre_3…
                let base = if st.record_name.trim().is_empty() {
                    chrono::Local::now()
                        .format("ruta_%Y-%m-%d_%H-%M")
                        .to_string()
                } else {
                    crate::model::sanitize(&st.record_name)
                };
                let mut name = base.clone();
                let mut n = 1;
                while Route::dir(&name).exists() {
                    n += 1;
                    name = format!("{base}_{n}");
                }
                let _ = std::fs::create_dir_all(Route::dir(&name));
                st.prev_route = std::mem::replace(
                    &mut st.route,
                    Route {
                        name: name.clone(),
                        ..Default::default()
                    },
                );
                // Posición real del cursor: si el primer clic llega sin haber movido el ratón,
                // no hay ningún MouseMove previo y se grabaría en (0, 0).
                if let mouse_position::mouse_position::Mouse::Position { x, y } =
                    mouse_position::mouse_position::Mouse::get_mouse_position()
                {
                    st.mouse = (x as f64, y as f64);
                }
                st.rec_id += 1;
                st.rec_target = None;
                st.rec_bad_anchors.clear();
                st.mode = Mode::Recording;
                st.last_step_at = Instant::now();
                drop(st);
                self.minimizar(true); // Replica se aparta: no estorba ni se graban clics sobre ella
                self.log(trf(
                    "● Grabando ruta '{}'. Pulsa F9 para terminar.",
                    &[&name],
                ));
            }
            Mode::Recording => {
                st.mode = Mode::Idle;
                clean_recording(&mut st.route.steps, from_gui);
                let route = st.route.clone();
                st.progress.clear();
                if route.steps.is_empty() {
                    st.route = std::mem::take(&mut st.prev_route);
                }
                drop(st);
                self.minimizar(false);
                if route.steps.is_empty() {
                    let _ = std::fs::remove_dir_all(Route::dir(&route.name));
                    self.log(tr("Grabación vacía: no se guarda nada."));
                    return self.repaint();
                }
                // Guardar cuando terminen sus anclas (en otro hilo: este puede ser el hook de teclado,
                // que no debe bloquearse). Mientras, no se puede reproducir ni grabar.
                self.saving.store(true, Ordering::SeqCst);
                let bot = self.clone();
                thread::spawn(move || bot.finish_recording(route));
                return;
            }
        }
        self.repaint();
    }

    /// Espera a las anclas pendientes, completa la ruta con la ventana objetivo
    /// y las anclas fallidas, y la guarda.
    fn finish_recording(self: &Arc<Self>, mut route: Route) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.pending.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        {
            let mut st = self.state.lock().unwrap();
            let (target, bad) = (
                st.rec_target.clone(),
                std::mem::take(&mut st.rec_bad_anchors),
            );
            finish_route(&mut route, target, &bad);
            recortar_esperas(&mut route.steps, st.settings.trim_wait_s);
            route.settings = Some(st.settings.clone());
            if st.route.name == route.name {
                // Conserva ediciones hechas en el editor mientras se guardaba.
                finish_route(&mut st.route, route.target.clone(), &bad);
            }
        }
        match route.save() {
            Ok(()) => self.log(trf(
                "■ Ruta '{}' guardada: {} pasos, {} s. {}",
                &[
                    &route.name,
                    &route.steps.len(),
                    &format!("{:.1}", route.duration_ms() as f64 / 1000.0),
                    &match &route.target {
                        Some(t) => trf("Ventana objetivo: {}", &[&t.label()]),
                        None => tr("Sin ventana objetivo: coordenadas de pantalla.").into(),
                    },
                ],
            )),
            Err(e) => self.log(trf("× No se pudo guardar la ruta: {}", &[&e])),
        }
        self.refresh_hotkeys();
        self.saving.store(false, Ordering::SeqCst);
        self.repaint();
    }

    // ───────────────────────── Reproducción ─────────────────────────

    pub fn toggle_play(self: &Arc<Self>) {
        let st = self.state.lock().unwrap();
        match st.mode {
            Mode::Playing => {
                drop(st);
                self.stop_all();
            }
            Mode::Recording => {
                drop(st);
                self.log(tr("Termina la grabación primero (F9)."));
            }
            Mode::Idle => {
                if self.saving.load(Ordering::SeqCst) {
                    drop(st);
                    return self.log(tr("Terminando de guardar la grabación…"));
                }
                if st.route.steps.is_empty() {
                    drop(st);
                    return self.log(tr("La ruta está vacía: graba o carga una."));
                }
                let commands = st.route.commands();
                if !st.commands_ok() {
                    drop(st);
                    return self.log(trf(
                        "Esta ruta ejecuta programas: {}. Revísalos y pulsa «permitir» antes de reproducir.",
                        &[&commands.join(" · ")],
                    ));
                }
                let (route, settings) = (st.route.clone(), st.settings.clone());
                drop(st);
                self.start_play(route, settings);
            }
        }
    }

    /// Reproduce una ruta que no es la abierta (recetas rápidas) sin tocar el editor.
    pub fn play_receta(self: &Arc<Self>, route: Route, settings: Settings) {
        self.start_play(route, settings);
    }

    fn start_play(self: &Arc<Self>, route: Route, settings: Settings) {
        {
            let mut st = self.state.lock().unwrap();
            if st.mode != Mode::Idle || self.is_saving() {
                drop(st);
                return self.log(trf(
                    "{}: '{}' no se lanza, Replica está ocupada.",
                    &[&tr("Reproducir"), &route.name],
                ));
            }
            st.mode = Mode::Playing;
        }
        {
            self.stop.store(false, Ordering::SeqCst);
            self.paused.store(false, Ordering::SeqCst);
            let bot = self.clone();
            thread::spawn(move || {
                let mut player = Player::new(&bot, &route, &settings);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    player.countdown(settings.countdown_s)?;
                    bot.log(trf(
                        "▶ Reproduciendo '{}' (F11 pausa · F12 para).",
                        &[&route.name],
                    ));
                    player.run()
                }));
                player.release_all();
                let aviso = match result {
                    Ok(Ok(())) => Some((true, trf("✔ '{}' completada.", &[&route.name]))),
                    // Parada a mano (F12, F10, cerrar): no hace falta avisar.
                    Ok(Err(_)) if bot.stopped() => None,
                    Ok(Err(e)) => Some((false, format!("× {e}"))),
                    Err(_) => Some((
                        false,
                        tr("× Error interno durante la reproducción.").to_string(),
                    )),
                };
                match &aviso {
                    Some((_, m)) => bot.log(m.clone()),
                    None => bot.log(tr("× Detenido por el usuario.")),
                }
                bot.paused.store(false, Ordering::SeqCst);
                let mut st = bot.state.lock().unwrap_or_else(|e| e.into_inner());
                st.aviso = aviso;
                st.mode = Mode::Idle;
                st.progress.clear();
                st.play = Progress::default();
                drop(st);
                bot.repaint();
            });
        }
    }

    /// F11: pausa o reanuda la reproducción en curso.
    pub fn toggle_pause(self: &Arc<Self>) {
        if self.mode() != Mode::Playing {
            return;
        }
        let now = !self.paused.fetch_xor(true, Ordering::SeqCst);
        self.log(if now {
            tr("En pausa (F11 para seguir).")
        } else {
            tr("▶ Reanudando.")
        });
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn is_saving(&self) -> bool {
        self.saving.load(Ordering::SeqCst)
    }

    /// Carga una ruta guardada como ruta actual (con sus ajustes, si los tiene).
    pub fn load_route(&self, name: &str) -> anyhow::Result<()> {
        let r = Route::load(name)?;
        let mut st = self.state.lock().unwrap();
        if let Some(cfg) = &r.settings {
            st.settings = cfg.clone();
        }
        st.route = r;
        st.step_cursor = 0;
        Ok(())
    }

    /// Guarda la ruta actual con los ajustes actuales.
    pub fn save_route(&self) -> anyhow::Result<()> {
        let route = {
            let mut st = self.state.lock().unwrap();
            if st.route.name.is_empty() {
                st.route.name = crate::model::sanitize(&st.record_name);
            }
            st.route.settings = Some(st.settings.clone());
            st.route.clone()
        };
        route.save()?;
        self.refresh_hotkeys();
        Ok(())
    }

    /// Guarda la configuración de la app (programaciones, máscara, permisos).
    pub fn save_config(&self) {
        let cfg = self.state.lock().unwrap().config.clone();
        if let Err(e) = cfg.save() {
            self.log(trf("× No se pudo guardar la configuración: {}", &[&e]));
        }
    }

    /// Relee qué ruta tiene asignado cada atajo F1–F8.
    pub fn refresh_hotkeys(&self) {
        #[derive(serde::Deserialize)]
        struct Head {
            #[serde(default)]
            settings: Option<Settings>,
        }
        let mut map = HashMap::new();
        for name in Route::list() {
            let path = Route::dir(&name).join("route.json");
            let head = std::fs::read_to_string(path)
                .ok()
                .and_then(|j| serde_json::from_str::<Head>(&j).ok());
            if let Some(k) = head.and_then(|h| h.settings).and_then(|s| s.hotkey) {
                map.insert(k, name);
            }
        }
        *self.route_hotkeys.lock().unwrap() = map;
    }

    pub fn hotkey_owner(&self, k: Key) -> Option<String> {
        self.route_hotkeys.lock().unwrap().get(&k).cloned()
    }

    /// Carga y reproduce una ruta guardada (desde un atajo o el programador).
    /// Nunca ejecuta programas que el usuario no haya permitido.
    pub fn play_named(self: &Arc<Self>, name: &str, origin: &str) {
        if self.mode() != Mode::Idle || self.is_saving() {
            return self.log(trf(
                "{}: '{}' no se lanza, Replica está ocupada.",
                &[&origin, &name],
            ));
        }
        if self.state.lock().unwrap().unsaved {
            return self.log(trf(
                "{}: '{}' no se lanza: la ruta abierta tiene cambios sin guardar.",
                &[&origin, &name],
            ));
        }
        if let Err(e) = self.load_route(name) {
            return self.log(trf(
                "× {}: no se pudo cargar '{}': {}",
                &[&origin, &name, &e],
            ));
        }
        if !self.state.lock().unwrap().commands_ok() {
            return self.log(trf(
                "{}: '{}' ejecuta programas sin permitir; ábrela y pulsa «permitir».",
                &[&origin, &name],
            ));
        }
        self.log(trf("{}: lanzando '{}'.", &[&origin, &name]));
        self.toggle_play();
    }

    /// Mira las programaciones y lanza la primera que toque.
    pub fn tick_schedules(self: &Arc<Self>, now: chrono::DateTime<chrono::Local>) {
        let mut due = vec![];
        {
            let mut st = self.state.lock().unwrap();
            for s in st.config.schedules.iter_mut() {
                if s.is_due(now) {
                    s.last = Some(now); // marca la ocasión aunque se salte por estar ocupado
                    due.push(s.route.clone());
                }
            }
        }
        if due.is_empty() {
            return;
        }
        self.save_config();
        for name in due {
            self.play_named(&name, tr("Programador")); // si coinciden varias, las demás se saltan
        }
    }

    /// Captura en 3 s lo que haya bajo el ratón como imagen de parada de la ruta abierta.
    pub fn capturar_until(self: &Arc<Self>, appear: bool) {
        let name = self.state.lock().unwrap().route.name.clone();
        if name.is_empty() || !Route::dir(&name).exists() {
            return self.log(tr("Guarda la ruta antes de capturar la imagen de parada."));
        }
        self.log(tr(
            "Pon el ratón sobre lo que hay que vigilar: se captura en 3 s.",
        ));
        let bot = self.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(3));
            let mouse_position::mouse_position::Mouse::Position { x, y } =
                mouse_position::mouse_position::Mouse::get_mouse_position()
            else {
                return bot.log(tr("× No se pudo leer la posición del ratón."));
            };
            let (x, y) = (x as f64, y as f64);
            let path = Route::dir(&name).join(UNTIL_FILE);
            match vision::capture_anchor(x, y).and_then(|img| Ok(img.save(&path)?)) {
                Ok(()) => {
                    bot.state.lock().unwrap().settings.until = Some(Until { x, y, appear });
                    bot.log(tr("Imagen de parada capturada (pulsa Guardar)."));
                }
                Err(e) => bot.log(trf("× No se pudo capturar: {}", &[&e])),
            }
            bot.repaint();
        });
    }

    /// Ejecuta solo el paso marcado por `step_cursor` y avanza al siguiente.
    pub fn step_once(self: &Arc<Self>) {
        let mut st = self.state.lock().unwrap();
        if st.mode != Mode::Idle || st.route.steps.is_empty() {
            return;
        }
        if !st.commands_ok() {
            drop(st);
            return self.log(tr("Esta ruta ejecuta programas: pulsa «permitir» antes."));
        }
        let i = st.step_cursor.min(st.route.steps.len() - 1);
        st.mode = Mode::Playing;
        let (route, settings) = (st.route.clone(), st.settings.clone());
        drop(st);
        self.stop.store(false, Ordering::SeqCst);
        let bot = self.clone();
        thread::spawn(move || {
            let mut player = Player::new(&bot, &route, &settings);
            let result = player
                .locate_window(true)
                .and_then(|_| player.exec(&route.steps[i].action));
            player.release_all(); // paso a paso no deja nada pulsado entre pasos
            match result {
                Ok(()) => bot.log(trf(
                    "Paso {} ejecutado: {}",
                    &[&(i + 1), &route.steps[i].action.describe()],
                )),
                Err(e) => bot.log(trf("× Paso {}: {}", &[&(i + 1), &e])),
            }
            let mut st = bot.state.lock().unwrap();
            st.mode = Mode::Idle;
            st.step_cursor = (i + 1) % route.steps.len();
            drop(st);
            bot.repaint();
        });
    }

    pub fn stop_all(self: &Arc<Self>) {
        self.stop.store(true, Ordering::SeqCst);
        if self.mode() == Mode::Recording {
            self.toggle_record(false);
        }
    }

    /// Al cerrar la app: para la reproducción y espera a que suelte teclas y botones,
    /// y a que se guarde una grabación en curso.
    pub fn shutdown(self: &Arc<Self>) {
        self.stop_all();
        let deadline = Instant::now() + Duration::from_secs(3);
        while (self.mode() == Mode::Playing || self.saving.load(Ordering::SeqCst))
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    fn set_progress(&self, play: Progress) {
        self.state.lock().unwrap().play = play;
        self.repaint();
    }
}

/// Recorta las esperas de más de `max_s` segundos (0 = no tocar) y quita los movimientos
/// de ratón del final, que no hacen nada.
fn recortar_esperas(steps: &mut Vec<Step>, max_s: u32) {
    if max_s > 0 {
        for s in steps.iter_mut() {
            s.delay_ms = s.delay_ms.min(max_s as u64 * 1000);
        }
    }
    while matches!(
        steps.last().map(|s| &s.action),
        Some(Action::MouseMove { .. })
    ) {
        steps.pop();
    }
}

/// Pone la ventana objetivo y quita las anclas que no se pudieron guardar
/// (sin PNG, la reproducción las trataría como ancla perdida).
fn finish_route(route: &mut Route, target: Option<Target>, bad_anchors: &[String]) {
    route.target = target;
    for s in &mut route.steps {
        if let Action::ButtonDown { anchor, .. } = &mut s.action {
            if anchor.as_ref().is_some_and(|a| bad_anchors.contains(a)) {
                *anchor = None;
            }
        }
    }
}

/// Quita de la grabación el ruido del principio/final (clics sobre la propia app).
fn clean_recording(steps: &mut Vec<Step>, from_gui: bool) {
    // Eventos "Up" sin su "Down" al principio (p.ej. soltar el botón Grabar).
    let mut held_buttons = HashSet::new();
    let mut held_keys = HashSet::new();
    steps.retain(|s| match &s.action {
        Action::ButtonDown { button, .. } => {
            held_buttons.insert(*button);
            true
        }
        Action::ButtonUp { button } => held_buttons.remove(button),
        Action::KeyDown { key } => {
            held_keys.insert(*key);
            true
        }
        Action::KeyUp { key } => held_keys.remove(key),
        _ => true,
    });
    if from_gui {
        // El último clic (pulsar + soltar) fue sobre el botón "Detener" de la app.
        let is_move = |s: &Step| matches!(s.action, Action::MouseMove { .. });
        while steps.last().is_some_and(is_move) {
            steps.pop();
        }
        if matches!(
            steps.last().map(|s| &s.action),
            Some(Action::ButtonUp { .. })
        ) {
            steps.pop();
        }
        while steps.last().is_some_and(is_move) {
            steps.pop();
        }
        if matches!(
            steps.last().map(|s| &s.action),
            Some(Action::ButtonDown { .. })
        ) {
            steps.pop();
        }
        while steps.last().is_some_and(is_move) {
            steps.pop();
        }
    }
}

/// Elimina movimientos de ratón intermedios (deja sólo los necesarios para
/// clics y arrastres). La ruta queda más corta y robusta.
pub fn simplify_moves(steps: &mut Vec<Step>) {
    let mut out: Vec<Step> = Vec::with_capacity(steps.len());
    let mut buttons_held = 0i32;
    let mut carry = 0u64;
    for (i, s) in steps.iter().enumerate() {
        let keep = match &s.action {
            Action::MouseMove { .. } => {
                let next_is_move = matches!(
                    steps.get(i + 1).map(|n| &n.action),
                    Some(Action::MouseMove { .. })
                );
                buttons_held > 0 || !next_is_move
            }
            Action::ButtonDown { .. } => {
                buttons_held += 1;
                true
            }
            Action::ButtonUp { .. } => {
                buttons_held = (buttons_held - 1).max(0);
                true
            }
            _ => true,
        };
        if keep {
            let mut s = s.clone();
            s.delay_ms += carry;
            carry = 0;
            out.push(s);
        } else {
            carry += s.delay_ms;
        }
    }
    *steps = out;
}

struct Player<'a> {
    bot: &'a Arc<Bot>,
    route: &'a Route,
    cfg: &'a Settings,
    anchors: HashMap<String, Option<RgbaImage>>,
    /// Ventana objetivo tal como está ahora (None = coordenadas grabadas tal cual).
    window_now: Option<Target>,
    keys_down: HashSet<Key>,
    buttons_down: Vec<Button>,
    rng: u64,
    /// Nivel de rutas encadenadas (para no entrar en un bucle infinito).
    depth: u32,
}

impl<'a> Player<'a> {
    fn new(bot: &'a Arc<Bot>, route: &'a Route, cfg: &'a Settings) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        Self {
            bot,
            route,
            cfg,
            anchors: HashMap::new(),
            window_now: None,
            keys_down: HashSet::new(),
            buttons_down: vec![],
            rng: seed | 1,
            depth: 0,
        }
    }

    /// Aleatorio en [-1, 1] (xorshift, suficiente para humanizar).
    fn rand(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % 20001) as f64 / 10000.0 - 1.0
    }

    /// Duración real de un retardo grabado (velocidad + humanización).
    fn scaled(&mut self, ms: u64) -> Duration {
        let mut ms = ms as f64 / self.cfg.speed.max(0.05) as f64;
        if self.cfg.humanize && ms > 5.0 {
            ms *= 1.0 + 0.15 * self.rand();
        }
        Duration::from_secs_f64(ms / 1000.0)
    }

    fn sleep(&mut self, ms: u64) -> anyhow::Result<()> {
        let mut end = Instant::now() + self.scaled(ms);
        self.sleep_until(&mut end)
    }

    fn countdown(&mut self, secs: u32) -> anyhow::Result<()> {
        for left in (1..=secs).rev() {
            self.bot.set_progress(Progress {
                countdown: left,
                ..Default::default()
            });
            let mut end = Instant::now() + Duration::from_secs(1);
            self.sleep_until(&mut end)?;
        }
        Ok(())
    }

    /// En pausa: suelta lo que esté pulsado (si no, el personaje sigue andando), espera,
    /// y al reanudar lo vuelve a pulsar. Devuelve cuánto duró la pausa.
    fn hold_while_paused(&mut self) -> anyhow::Result<Duration> {
        if !self.bot.is_paused() {
            return Ok(Duration::ZERO);
        }
        let t0 = Instant::now();
        let (keys, buttons) = (self.keys_down.clone(), self.buttons_down.clone());
        for k in &keys {
            self.send(EventType::KeyRelease(*k));
        }
        for b in &buttons {
            self.send(EventType::ButtonRelease(*b));
        }
        while self.bot.is_paused() {
            if self.bot.stopped() {
                anyhow::bail!("{}", tr("Detenido por el usuario."));
            }
            thread::sleep(Duration::from_millis(20));
        }
        for k in &keys {
            self.send(EventType::KeyPress(*k));
        }
        for b in &buttons {
            self.send(EventType::ButtonPress(*b));
        }
        Ok(t0.elapsed())
    }

    /// Espera hasta `end` comprobando F12. Comprueba aunque no haya que esperar:
    /// si no, una ruta de pasos sin retardo nunca vería la parada.
    /// La pausa alarga `end` lo que dure, para no disparar después los pasos atrasados.
    fn sleep_until(&mut self, end: &mut Instant) -> anyhow::Result<()> {
        loop {
            if self.bot.stopped() {
                anyhow::bail!("{}", tr("Detenido por el usuario."));
            }
            *end += self.hold_while_paused()?;
            let now = Instant::now();
            if now >= *end {
                return Ok(());
            }
            thread::sleep((*end - now).min(Duration::from_millis(20)));
        }
    }

    fn send(&self, ev: EventType) {
        if let Err(e) = rdev::simulate(&ev) {
            self.bot.log(trf(
                "× No se pudo simular {}: {}",
                &[&format!("{ev:?}"), &format!("{e:?}")],
            ));
        }
        // Algunos sistemas pierden eventos si llegan demasiado juntos.
        thread::sleep(Duration::from_millis(2));
    }

    fn run(&mut self) -> anyhow::Result<()> {
        let (mut lap, mut fallos) = (0u32, 0u32);
        loop {
            lap += 1;
            if let Err(e) = self.vuelta(lap) {
                // Reintento: repetir la vuelta desde el principio (si no fue el usuario quien paró).
                if self.bot.stopped() || fallos >= self.cfg.retries {
                    return Err(e);
                }
                fallos += 1;
                self.release_all();
                self.bot.log(trf(
                    "Reintento {} de {}: {}",
                    &[&fallos, &self.cfg.retries, &e],
                ));
                lap -= 1;
                self.sleep(1000)?;
                continue;
            }
            if self.condicion_parada()? {
                self.bot.log(tr("■ Se cumplió la condición de parada."));
                return Ok(());
            }
            if self.cfg.loops != 0 && lap >= self.cfg.loops {
                return Ok(());
            }
            self.sleep(self.cfg.pause_between_loops_ms)?;
        }
    }

    /// Una vuelta completa de la ruta.
    fn vuelta(&mut self, lap: u32) -> anyhow::Result<()> {
        self.locate_window(lap == 1)?;
        let mut clock = Instant::now();
        let steps = self.route.steps.len();
        for (i, step) in self.route.steps.iter().enumerate() {
            if self.depth == 0 {
                self.bot.set_progress(Progress {
                    lap,
                    laps: self.cfg.loops,
                    step: i + 1,
                    steps,
                    countdown: 0,
                });
            }
            clock += self.scaled(step.delay_ms);
            self.sleep_until(&mut clock)?;
            self.exec(&step.action)?;
            // Reloj absoluto: lo que tarda simular eventos no se acumula como retraso, pero tras
            // una espera larga (ancla, Wait) no se disparan de golpe los pasos atrasados.
            let catch_up = Instant::now()
                .checked_sub(Duration::from_millis(50))
                .unwrap_or(clock);
            clock = clock.max(catch_up);
        }
        self.release_all();
        Ok(())
    }

    /// «Repetir hasta que…»: ¿apareció (o desapareció) la imagen de parada?
    fn condicion_parada(&mut self) -> anyhow::Result<bool> {
        let Some(u) = self.cfg.until.clone() else {
            return Ok(false);
        };
        let path = self.route.anchor_path(UNTIL_FILE);
        let Ok(img) = image::open(&path) else {
            return Ok(false);
        };
        let tpl = image::imageops::grayscale(&img.to_rgba8());
        let (x, y) = self.map(u.x, u.y);
        let visible = vision::find_near(&[tpl], x, y, self.cfg.search_radius)
            .map(|m| m.score >= self.cfg.threshold)
            .unwrap_or(false);
        Ok(visible == u.appear)
    }

    fn exec(&mut self, action: &Action) -> anyhow::Result<()> {
        match action {
            Action::MouseMove { x, y } => {
                let (x, y) = self.map(*x, *y);
                self.send(EventType::MouseMove { x, y })
            }
            Action::ButtonDown {
                button,
                x,
                y,
                anchor,
            } => {
                self.locate_window(false)?; // la ventana puede haberse movido a mitad de vuelta
                let (mut px, mut py) = self.map(*x, *y);
                if self.bot.stopped() {
                    anyhow::bail!("{}", tr("Detenido por el usuario."));
                }
                if self.cfg.use_anchors {
                    if let Some(file) = anchor {
                        (px, py) = self.locate_anchor(file, px, py)?;
                    }
                }
                if self.cfg.humanize {
                    px += 2.0 * self.rand();
                    py += 2.0 * self.rand();
                }
                self.send(EventType::MouseMove { x: px, y: py });
                self.send(EventType::ButtonPress(*button));
                self.buttons_down.push(*button);
            }
            Action::ButtonUp { button } => {
                self.send(EventType::ButtonRelease(*button));
                self.buttons_down.retain(|b| b != button);
            }
            Action::Wheel { dx, dy } => self.send(EventType::Wheel {
                delta_x: *dx,
                delta_y: *dy,
            }),
            Action::KeyDown { key } => {
                self.send(EventType::KeyPress(*key));
                self.keys_down.insert(*key);
            }
            Action::KeyUp { key } => {
                self.send(EventType::KeyRelease(*key));
                self.keys_down.remove(key);
            }
            Action::TypeText { text } => {
                for c in text.chars() {
                    if self.bot.stopped() {
                        anyhow::bail!("{}", tr("Detenido por el usuario."));
                    }
                    // Letras, números, espacio y Enter con teclas físicas (los juegos leen teclas);
                    // símbolos, tildes, ñ… como Unicode, que no depende de la distribución del teclado.
                    if !(c.is_ascii_alphanumeric() || matches!(c, ' ' | '\n' | '\t')) {
                        if let Err(e) = type_unicode(c) {
                            self.bot.log(trf(
                                "× No se pudo escribir {}: {}",
                                &[&format!("{c:?}"), &e],
                            ));
                        }
                        thread::sleep(Duration::from_millis(15));
                        continue;
                    }
                    match char_to_key(c) {
                        Some((key, shift)) => {
                            if shift {
                                self.send(EventType::KeyPress(Key::ShiftLeft));
                            }
                            self.send(EventType::KeyPress(key));
                            self.send(EventType::KeyRelease(key));
                            if shift {
                                self.send(EventType::KeyRelease(Key::ShiftLeft));
                            }
                            thread::sleep(Duration::from_millis(15));
                        }
                        None => self.bot.log(trf(
                            "Carácter no soportado al escribir: {}",
                            &[&format!("{c:?}")],
                        )),
                    }
                }
            }
            Action::RunCommand { command } => {
                self.bot.log(trf("Ejecutando: {}", &[command]));
                #[cfg(windows)]
                let r = std::process::Command::new("cmd")
                    .args(["/C", command])
                    .spawn();
                #[cfg(not(windows))]
                let r = std::process::Command::new("sh")
                    .args(["-c", command])
                    .spawn();
                match r {
                    Ok(mut child) => drop(thread::spawn(move || child.wait())),
                    Err(e) => self
                        .bot
                        .log(trf("× Error al ejecutar '{}': {}", &[command, &e])),
                }
            }
            Action::Wait { ms } => self.sleep(*ms)?,
            Action::ClickHere { button } => {
                self.send(EventType::ButtonPress(*button));
                thread::sleep(Duration::from_millis(30));
                self.send(EventType::ButtonRelease(*button));
            }
            Action::RunRoute { name } => {
                if self.depth >= 4 {
                    anyhow::bail!("{}", trf("Demasiadas rutas encadenadas en '{}'.", &[name]));
                }
                let sub = Route::load(name).map_err(|e| {
                    anyhow::anyhow!("{}", trf("No se pudo cargar '{}': {}", &[name, &e]))
                })?;
                if !self.bot.state.lock().unwrap().config.is_trusted(&sub) {
                    anyhow::bail!("{}", trf("'{}' ejecuta programas sin permitir.", &[name]));
                }
                // Con los ajustes de la ruta encadenada (los suyos), una sola vuelta.
                let cfg = Settings {
                    loops: 1,
                    ..sub.settings.clone().unwrap_or_else(|| self.cfg.clone())
                };
                let mut p = Player::new(self.bot, &sub, &cfg);
                p.depth = self.depth + 1;
                let r = p.vuelta(1);
                p.release_all();
                r?;
            }
        }
        Ok(())
    }

    /// Busca la ventana objetivo y avisa si cambió de tamaño. Si no está, aplica la
    /// misma política que con un ancla perdida (detener o seguir con las coordenadas grabadas).
    fn locate_window(&mut self, first: bool) -> anyhow::Result<()> {
        let Some(rec) = &self.route.target else {
            return Ok(());
        };
        let now = vision::find_window(rec);
        if now.is_none() && self.cfg.on_missing == MissingAnchor::Stop {
            anyhow::bail!(
                "{}",
                trf(
                    "No encuentro la ventana de '{}'. Ruta detenida.",
                    &[&rec.app]
                )
            );
        }
        if first || now != self.window_now {
            let msg = match &now {
                Some(n) if n == rec => trf("Ventana objetivo: {}", &[&n.label()]),
                Some(n) => {
                    let (sx, sy) = rec.scale_to(n);
                    trf(
                        "Ventana en ({}, {}) {}×{} (grabada {}×{}): escala {}×{}.",
                        &[
                            &n.x,
                            &n.y,
                            &n.w,
                            &n.h,
                            &rec.w,
                            &rec.h,
                            &format!("{sx:.2}"),
                            &format!("{sy:.2}"),
                        ],
                    )
                }
                None => trf(
                    "No encuentro la ventana de '{}': uso las coordenadas grabadas.",
                    &[&rec.app],
                ),
            };
            self.bot.log(msg);
        }
        self.window_now = now;
        Ok(())
    }

    /// Coordenadas grabadas → coordenadas actuales según la ventana objetivo.
    fn map(&self, x: f64, y: f64) -> (f64, f64) {
        match (&self.route.target, &self.window_now) {
            (Some(rec), Some(now)) => rec.map_to(now, x, y),
            _ => (x, y),
        }
    }

    fn scale(&self) -> (f64, f64) {
        match (&self.route.target, &self.window_now) {
            (Some(rec), Some(now)) => rec.scale_to(now),
            _ => (1.0, 1.0),
        }
    }

    /// Espera a que el ancla aparezca en pantalla y devuelve dónde clicar.
    /// `x, y` ya están en coordenadas actuales.
    fn locate_anchor(&mut self, file: &str, x: f64, y: f64) -> anyhow::Result<(f64, f64)> {
        let path = self.route.anchor_path(file);
        let tpl = self
            .anchors
            .entry(file.to_string())
            .or_insert_with(|| image::open(&path).ok().map(|i| i.to_rgba8()))
            .clone();
        let Some(tpl) = tpl else {
            return self.anchor_missing(&trf("No se puede leer el ancla {}", &[&file]), x, y);
        };
        // Otra resolución: los juegos escalan su interfaz con la ventana y las apps de
        // escritorio no, así que se prueba el ancla escalada y la original.
        let (sx, sy) = self.scale();
        let tpl = image::imageops::grayscale(&tpl);
        let mut tpls = vec![];
        if (sx - 1.0).abs() > 0.01 || (sy - 1.0).abs() > 0.01 {
            let w = ((tpl.width() as f64 * sx).round() as u32).max(4);
            let h = ((tpl.height() as f64 * sy).round() as u32).max(4);
            tpls.push(image::imageops::resize(
                &tpl,
                w,
                h,
                image::imageops::FilterType::Triangle,
            ));
        }
        tpls.push(tpl);
        let radius = (self.cfg.search_radius as f64 * sx.max(sy)) as u32;
        let deadline = Instant::now() + Duration::from_millis(self.cfg.anchor_timeout_ms);
        let mut best = 0.0f32;
        loop {
            match vision::find_near(&tpls, x, y, radius) {
                Ok(m) if m.score >= self.cfg.threshold => {
                    if (m.x - x).abs() > 3.0 || (m.y - y).abs() > 3.0 {
                        self.bot.log(trf(
                            "Ancla desplazada: {} → {} [{}]",
                            &[
                                &format!("({x:.0},{y:.0})"),
                                &format!("({:.0},{:.0})", m.x, m.y),
                                &format!("{:.2}", m.score),
                            ],
                        ));
                    }
                    return Ok((m.x, m.y));
                }
                Ok(m) => best = best.max(m.score),
                Err(e) => {
                    return self.anchor_missing(
                        &trf("Captura de pantalla falló ({})", &[&e]),
                        x,
                        y,
                    );
                }
            }
            if Instant::now() >= deadline {
                let why = trf(
                    "Ancla {} no apareció en pantalla (mejor {})",
                    &[&file, &format!("{best:.2}")],
                );
                return self.anchor_missing(&why, x, y);
            }
            if self.bot.stopped() {
                anyhow::bail!("{}", tr("Detenido por el usuario."));
            }
            thread::sleep(Duration::from_millis(150));
        }
    }

    /// Política única para cualquier fallo de verificación visual: con «detener»
    /// nunca se clica sin verificar.
    fn anchor_missing(&self, why: &str, x: f64, y: f64) -> anyhow::Result<(f64, f64)> {
        match self.cfg.on_missing {
            MissingAnchor::Stop => anyhow::bail!("{}", trf("{}. Ruta detenida.", &[&why])),
            MissingAnchor::ClickAnyway => {
                self.bot
                    .log(trf("{}; clic en la posición grabada.", &[&why]));
                Ok((x, y))
            }
        }
    }

    /// Suelta todo lo que haya quedado pulsado (evita teclas "pegadas").
    fn release_all(&mut self) {
        for k in std::mem::take(&mut self.keys_down) {
            self.send(EventType::KeyRelease(k));
        }
        for b in std::mem::take(&mut self.buttons_down) {
            self.send(EventType::ButtonRelease(b));
        }
    }
}

/// Escribe un carácter como Unicode (SendInput en Windows, XTest con remapeo en X11, CGEvent en macOS).
fn type_unicode(c: char) -> anyhow::Result<()> {
    use enigo::{Enigo, Keyboard};
    let mut enigo = Enigo::new(&enigo::Settings::default()).map_err(|e| anyhow::anyhow!("{e}"))?;
    enigo
        .text(&c.to_string())
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Mapeo carácter → tecla física (distribución US). Devuelve (tecla, necesita Shift).
pub fn char_to_key(c: char) -> Option<(Key, bool)> {
    use Key::*;
    const LETTERS: [Key; 26] = [
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO,
        KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    ];
    const DIGITS: [Key; 10] = [Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9];
    const SHIFT_DIGITS: &str = ")!@#$%^&*(";
    Some(match c {
        'a'..='z' => (LETTERS[c as usize - 'a' as usize], false),
        'A'..='Z' => (LETTERS[c as usize - 'A' as usize], true),
        '0'..='9' => (DIGITS[c as usize - '0' as usize], false),
        ' ' => (Space, false),
        '\n' => (Return, false),
        '\t' => (Tab, false),
        '-' => (Minus, false),
        '_' => (Minus, true),
        '=' => (Equal, false),
        '+' => (Equal, true),
        '[' => (LeftBracket, false),
        '{' => (LeftBracket, true),
        ']' => (RightBracket, false),
        '}' => (RightBracket, true),
        ';' => (SemiColon, false),
        ':' => (SemiColon, true),
        '\'' => (Quote, false),
        '"' => (Quote, true),
        ',' => (Comma, false),
        '<' => (Comma, true),
        '.' => (Dot, false),
        '>' => (Dot, true),
        '/' => (Slash, false),
        '?' => (Slash, true),
        '\\' => (BackSlash, false),
        '|' => (BackSlash, true),
        '`' => (BackQuote, false),
        '~' => (BackQuote, true),
        _ => {
            let i = SHIFT_DIGITS.find(c)?;
            (DIGITS[i], true)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(d: u64) -> Step {
        Step {
            delay_ms: d,
            action: Action::MouseMove { x: 1.0, y: 1.0 },
        }
    }

    #[test]
    fn simplify_keeps_last_move_and_sums_delays() {
        let mut steps = vec![
            mv(10),
            mv(10),
            mv(10),
            Step {
                delay_ms: 5,
                action: Action::KeyDown { key: Key::KeyA },
            },
        ];
        simplify_moves(&mut steps);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].delay_ms, 30);
        assert_eq!(steps[1].delay_ms, 5);
    }

    #[test]
    fn clean_drops_orphan_release() {
        let mut steps = vec![
            Step {
                delay_ms: 0,
                action: Action::ButtonUp {
                    button: Button::Left,
                },
            },
            Step {
                delay_ms: 0,
                action: Action::KeyDown { key: Key::KeyW },
            },
            Step {
                delay_ms: 0,
                action: Action::KeyUp { key: Key::KeyW },
            },
        ];
        clean_recording(&mut steps, false);
        assert_eq!(steps.len(), 2);
    }

    #[test]
    fn clean_from_gui_drops_stop_click() {
        let down = |b| Step {
            delay_ms: 0,
            action: Action::ButtonDown {
                button: b,
                x: 0.0,
                y: 0.0,
                anchor: None,
            },
        };
        let up = |b| Step {
            delay_ms: 0,
            action: Action::ButtonUp { button: b },
        };
        let mut steps = vec![
            down(Button::Right),
            up(Button::Right),
            mv(5),
            down(Button::Left),
            mv(1),
            up(Button::Left),
        ];
        clean_recording(&mut steps, true);
        assert_eq!(steps.len(), 2);
    }

    fn route(steps: Vec<Action>, target: Option<Target>) -> Route {
        Route {
            name: "__test_inexistente__".into(),
            target,
            settings: None,
            steps: steps
                .into_iter()
                .map(|action| Step {
                    delay_ms: 0,
                    action,
                })
                .collect(),
        }
    }

    /// Ejecuta la ruta en otro hilo y devuelve su resultado, o None si no termina en 3 s.
    fn run_with_timeout(bot: &Arc<Bot>, r: Route, cfg: Settings) -> Option<anyhow::Result<()>> {
        let (tx, rx) = mpsc::channel();
        let bot = bot.clone();
        thread::spawn(move || {
            let _ = tx.send(Player::new(&bot, &r, &cfg).run());
        });
        rx.recv_timeout(Duration::from_secs(3)).ok()
    }

    #[test]
    fn trim_caps_long_waits_and_drops_trailing_moves() {
        let mut steps = vec![
            Step {
                delay_ms: 90_000,
                action: Action::KeyDown { key: Key::KeyW },
            },
            Step {
                delay_ms: 500,
                action: Action::KeyUp { key: Key::KeyW },
            },
            mv(10),
            mv(10),
        ];
        recortar_esperas(&mut steps, 5);
        assert_eq!(steps.len(), 2, "sin movimientos al final");
        assert_eq!(steps[0].delay_ms, 5_000);
        assert_eq!(steps[1].delay_ms, 500);
    }

    #[test]
    fn retries_repeat_the_loop_then_give_up() {
        let bot = Bot::new();
        // Encadenar una ruta que no existe: falla siempre.
        let r = route(
            vec![Action::RunRoute {
                name: "__no_existe__".into(),
            }],
            None,
        );
        let cfg = Settings {
            retries: 2,
            speed: 4.0,
            humanize: false,
            ..Default::default()
        };
        let result = run_with_timeout(&bot, r, cfg).expect("no terminó");
        assert!(result.is_err());
        let log = bot
            .state
            .lock()
            .unwrap()
            .log
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            log.contains("Reintento 1 de 2") && log.contains("Reintento 2 de 2"),
            "{log}"
        );
        assert!(!log.contains("Reintento 3"));
    }

    #[test]
    fn record_refuses_with_unsaved_changes() {
        let bot = Bot::new();
        bot.state.lock().unwrap().unsaved = true;
        bot.toggle_record(false);
        assert_eq!(
            bot.mode(),
            Mode::Idle,
            "F9 no debe pisar una ruta con cambios sin guardar"
        );
    }

    #[test]
    fn stop_works_without_any_delay() {
        // Bucle infinito de pasos sin retardo: antes F12 nunca se comprobaba y no paraba.
        let bot = Bot::new();
        bot.stop.store(true, Ordering::SeqCst);
        let cfg = Settings {
            loops: 0,
            pause_between_loops_ms: 0,
            humanize: false,
            ..Default::default()
        };
        let r = route(vec![Action::Wait { ms: 0 }], None);
        let result = run_with_timeout(&bot, r, cfg).expect("la reproducción no se detuvo");
        assert!(result.is_err());
    }

    #[test]
    fn unreadable_anchor_respects_stop_policy() {
        let bot = Bot::new();
        let r = route(vec![], None);
        let stop = Settings {
            on_missing: MissingAnchor::Stop,
            ..Default::default()
        };
        assert!(
            Player::new(&bot, &r, &stop)
                .locate_anchor("falta.png", 10.0, 20.0)
                .is_err()
        );
        let click = Settings {
            on_missing: MissingAnchor::ClickAnyway,
            ..Default::default()
        };
        let pos = Player::new(&bot, &r, &click)
            .locate_anchor("falta.png", 10.0, 20.0)
            .unwrap();
        assert_eq!(pos, (10.0, 20.0));
    }

    #[test]
    fn missing_target_window_respects_stop_policy() {
        let bot = Bot::new();
        let target = Target {
            app: "__app_inexistente__".into(),
            w: 100,
            h: 100,
            ..Default::default()
        };
        let r = route(vec![Action::Wait { ms: 0 }], Some(target));
        let cfg = Settings {
            on_missing: MissingAnchor::Stop,
            ..Default::default()
        };
        let result = run_with_timeout(&bot, r, cfg).expect("no terminó");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No encuentro la ventana")
        );
    }

    #[test]
    fn finish_route_sets_target_and_drops_failed_anchors() {
        let click = |a: &str| Action::ButtonDown {
            button: Button::Left,
            x: 0.0,
            y: 0.0,
            anchor: Some(a.into()),
        };
        let mut r = route(
            vec![click("anchor_0000.png"), click("anchor_0002.png")],
            None,
        );
        let t = Target {
            app: "juego".into(),
            ..Default::default()
        };
        finish_route(&mut r, Some(t.clone()), &["anchor_0002.png".into()]);
        assert_eq!(r.target, Some(t));
        let anchors: Vec<_> = r
            .steps
            .iter()
            .map(|s| match &s.action {
                Action::ButtonDown { anchor, .. } => anchor.clone(),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(anchors, vec![Some("anchor_0000.png".to_string()), None]);
    }

    #[test]
    fn chars_map() {
        assert_eq!(char_to_key('A'), Some((Key::KeyA, true)));
        assert_eq!(char_to_key('/'), Some((Key::Slash, false)));
        assert_eq!(char_to_key('!'), Some((Key::Num1, true)));
    }
}
