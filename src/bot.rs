//! Núcleo del bot: escucha global de ratón/teclado (grabación + atajos),
//! y reproductor de rutas con verificación visual.

use crate::model::{Action, Route, Step};
use crate::vision;
use image::RgbaImage;
use rdev::{Button, EventType, Key};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const HOTKEY_RECORD: Key = Key::F9;
pub const HOTKEY_PLAY: Key = Key::F10;
pub const HOTKEY_STOP: Key = Key::F12;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    Idle,
    Recording,
    Playing,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MissingAnchor {
    /// Si el ancla no aparece a tiempo, clica en la posición grabada.
    ClickAnyway,
    /// Si el ancla no aparece a tiempo, detiene la ruta (más seguro).
    Stop,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub speed: f32,
    /// 0 = infinito.
    pub loops: u32,
    pub pause_between_loops_ms: u64,
    pub humanize: bool,
    pub use_anchors: bool,
    pub threshold: f32,
    pub anchor_timeout_ms: u64,
    pub search_radius: u32,
    pub on_missing: MissingAnchor,
    pub record_moves: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            speed: 1.0,
            loops: 1,
            pause_between_loops_ms: 1000,
            humanize: true,
            use_anchors: true,
            threshold: 0.90,
            anchor_timeout_ms: 5000,
            search_radius: 120,
            on_missing: MissingAnchor::ClickAnyway,
            record_moves: true,
        }
    }
}

pub struct State {
    pub mode: Mode,
    /// Ruta cargada/en edición (la que se reproduce).
    pub route: Route,
    pub settings: Settings,
    pub record_name: String,
    pub log: VecDeque<String>,
    pub progress: String,
    mouse: (f64, f64),
    last_step_at: Instant,
    last_move_at: Instant,
    /// Atajo mantenido pulsado: ignora la autorrepetición del SO.
    hotkey_held: Option<Key>,
    /// Ruta cargada antes de grabar: se recupera si la grabación sale vacía.
    prev_route: Route,
}

pub struct Bot {
    pub state: Mutex<State>,
    stop: AtomicBool,
    repaint: Mutex<Option<eframe::egui::Context>>,
    anchor_tx: Mutex<mpsc::Sender<(f64, f64, std::path::PathBuf)>>,
}

impl Bot {
    pub fn new() -> Arc<Bot> {
        let (tx, rx) = mpsc::channel::<(f64, f64, std::path::PathBuf)>();
        let bot = Arc::new(Bot {
            state: Mutex::new(State {
                mode: Mode::Idle,
                route: Route { name: "nueva_ruta".into(), steps: vec![] },
                settings: Settings::default(),
                record_name: "nueva_ruta".into(),
                log: VecDeque::new(),
                progress: String::new(),
                mouse: (0.0, 0.0),
                last_step_at: Instant::now(),
                last_move_at: Instant::now(),
                hotkey_held: None,
                prev_route: Route::default(),
            }),
            stop: AtomicBool::new(false),
            repaint: Mutex::new(None),
            anchor_tx: Mutex::new(tx),
        });

        // Hilo que guarda anclas visuales sin bloquear la escucha de entrada.
        let b = bot.clone();
        thread::spawn(move || {
            for (x, y, path) in rx {
                match vision::capture_anchor(x, y).and_then(|img| Ok(img.save(&path)?)) {
                    Ok(()) => {}
                    Err(e) => b.log(format!("No se pudo capturar ancla: {e}")),
                }
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
            EventType::KeyPress(k @ (HOTKEY_RECORD | HOTKEY_PLAY | HOTKEY_STOP)) => {
                if self.state.lock().unwrap().hotkey_held.replace(k) == Some(k) {
                    return; // autorrepetición: si no, mantener F9 alterna grabar/parar sin control
                }
                return match k {
                    HOTKEY_RECORD => self.toggle_record(false),
                    HOTKEY_PLAY => self.toggle_play(),
                    _ => self.stop_all(),
                };
            }
            EventType::KeyRelease(HOTKEY_RECORD | HOTKEY_PLAY | HOTKEY_STOP) => {
                self.state.lock().unwrap().hotkey_held = None;
                return;
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
                if !st.settings.record_moves || now.duration_since(st.last_move_at) < Duration::from_millis(15) {
                    return;
                }
                st.last_move_at = now;
                Action::MouseMove { x, y }
            }
            EventType::ButtonPress(button) => {
                let (x, y) = st.mouse;
                let file = format!("anchor_{:04}.png", st.route.steps.len());
                let path = Route::dir(&st.route.name).join(&file);
                let _ = self.anchor_tx.lock().unwrap().send((x, y, path));
                Action::ButtonDown { button, x, y, anchor: Some(file) }
            }
            EventType::ButtonRelease(button) => Action::ButtonUp { button },
            EventType::Wheel { delta_x, delta_y } => Action::Wheel { dx: delta_x, dy: delta_y },
            EventType::KeyPress(key) => Action::KeyDown { key },
            EventType::KeyRelease(key) => Action::KeyUp { key },
        };
        let delay = if st.route.steps.is_empty() { 0 } else { now.duration_since(st.last_step_at).as_millis() as u64 };
        st.last_step_at = now;
        st.route.steps.push(Step { delay_ms: delay, action });
        let n = st.route.steps.len();
        st.progress = format!("Grabando… {n} pasos");
        drop(st);
        self.repaint();
    }

    // ───────────────────────── Grabación ─────────────────────────

    pub fn toggle_record(self: &Arc<Self>, from_gui: bool) {
        let mut st = self.state.lock().unwrap();
        match st.mode {
            Mode::Playing => {
                drop(st);
                self.log("Para la reproducción antes de grabar (F12).");
            }
            Mode::Idle => {
                // Nunca pisar una ruta existente: nombre, nombre_2, nombre_3…
                let base = crate::model::sanitize(&st.record_name);
                let mut name = base.clone();
                let mut n = 1;
                while Route::dir(&name).exists() {
                    n += 1;
                    name = format!("{base}_{n}");
                }
                let _ = std::fs::create_dir_all(Route::dir(&name));
                st.prev_route = std::mem::replace(&mut st.route, Route { name: name.clone(), steps: vec![] });
                st.mode = Mode::Recording;
                st.last_step_at = Instant::now();
                drop(st);
                self.log(format!("● Grabando ruta '{name}'. Pulsa F9 para terminar."));
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
                if route.steps.is_empty() {
                    let _ = std::fs::remove_dir_all(Route::dir(&route.name));
                    self.log("Grabación vacía: no se guarda nada.");
                    return self.repaint();
                }
                // Los PNG de anclas los sigue escribiendo su hilo; route.json no depende de ellos.
                match route.save() {
                    Ok(()) => self.log(format!(
                        "■ Ruta '{}' guardada: {} pasos, {:.1} s.",
                        route.name,
                        route.steps.len(),
                        route.duration_ms() as f64 / 1000.0
                    )),
                    Err(e) => self.log(format!("Error guardando ruta: {e}")),
                }
            }
        }
        self.repaint();
    }

    // ───────────────────────── Reproducción ─────────────────────────

    pub fn toggle_play(self: &Arc<Self>) {
        let mut st = self.state.lock().unwrap();
        match st.mode {
            Mode::Playing => {
                drop(st);
                self.stop_all();
            }
            Mode::Recording => {
                drop(st);
                self.log("Termina la grabación primero (F9).");
            }
            Mode::Idle => {
                if st.route.steps.is_empty() {
                    drop(st);
                    return self.log("La ruta está vacía: graba o carga una.");
                }
                st.mode = Mode::Playing;
                let route = st.route.clone();
                let settings = st.settings.clone();
                drop(st);
                self.stop.store(false, Ordering::SeqCst);
                let bot = self.clone();
                thread::spawn(move || {
                    bot.log(format!("▶ Reproduciendo '{}' (F12 para parar).", route.name));
                    let mut player = Player::new(&bot, &route, &settings);
                    let result = player.run();
                    player.release_all();
                    match result {
                        Ok(()) => bot.log("✔ Ruta completada."),
                        Err(e) => bot.log(format!("⏹ {e}")),
                    }
                    let mut st = bot.state.lock().unwrap();
                    st.mode = Mode::Idle;
                    st.progress.clear();
                    drop(st);
                    bot.repaint();
                });
            }
        }
    }

    pub fn stop_all(self: &Arc<Self>) {
        self.stop.store(true, Ordering::SeqCst);
        if self.mode() == Mode::Recording {
            self.toggle_record(false);
        }
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    fn set_progress(&self, p: String) {
        self.state.lock().unwrap().progress = p;
        self.repaint();
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
        if matches!(steps.last().map(|s| &s.action), Some(Action::ButtonUp { .. })) {
            steps.pop();
        }
        while steps.last().is_some_and(is_move) {
            steps.pop();
        }
        if matches!(steps.last().map(|s| &s.action), Some(Action::ButtonDown { .. })) {
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
                let next_is_move = matches!(steps.get(i + 1).map(|n| &n.action), Some(Action::MouseMove { .. }));
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
    keys_down: HashSet<Key>,
    buttons_down: Vec<Button>,
    rng: u64,
}

impl<'a> Player<'a> {
    fn new(bot: &'a Arc<Bot>, route: &'a Route, cfg: &'a Settings) -> Self {
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64;
        Self { bot, route, cfg, anchors: HashMap::new(), keys_down: HashSet::new(), buttons_down: vec![], rng: seed | 1 }
    }

    /// Aleatorio en [-1, 1] (xorshift, suficiente para humanizar).
    fn rand(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % 20001) as f64 / 10000.0 - 1.0
    }

    fn sleep(&mut self, ms: u64) -> anyhow::Result<()> {
        let mut ms = ms as f64 / self.cfg.speed.max(0.05) as f64;
        if self.cfg.humanize && ms > 5.0 {
            ms *= 1.0 + 0.15 * self.rand();
        }
        let end = Instant::now() + Duration::from_millis(ms as u64);
        while Instant::now() < end {
            if self.bot.stopped() {
                anyhow::bail!("Detenido por el usuario.");
            }
            thread::sleep((end - Instant::now()).min(Duration::from_millis(20)));
        }
        Ok(())
    }

    fn send(&self, ev: EventType) {
        if let Err(e) = rdev::simulate(&ev) {
            self.bot.log(format!("No se pudo simular {ev:?}: {e:?}"));
        }
        // Algunos sistemas pierden eventos si llegan demasiado juntos.
        thread::sleep(Duration::from_millis(2));
    }

    fn run(&mut self) -> anyhow::Result<()> {
        let mut lap = 0u32;
        loop {
            lap += 1;
            let total = if self.cfg.loops == 0 { "∞".to_string() } else { self.cfg.loops.to_string() };
            for (i, step) in self.route.steps.iter().enumerate() {
                self.bot.set_progress(format!("Vuelta {lap}/{total} · paso {}/{}", i + 1, self.route.steps.len()));
                self.sleep(step.delay_ms)?;
                self.exec(&step.action)?;
            }
            self.release_all();
            if self.cfg.loops != 0 && lap >= self.cfg.loops {
                return Ok(());
            }
            self.sleep(self.cfg.pause_between_loops_ms)?;
        }
    }

    fn exec(&mut self, action: &Action) -> anyhow::Result<()> {
        match action {
            Action::MouseMove { x, y } => self.send(EventType::MouseMove { x: *x, y: *y }),
            Action::ButtonDown { button, x, y, anchor } => {
                let (mut px, mut py) = (*x, *y);
                if self.cfg.use_anchors {
                    if let Some(file) = anchor {
                        (px, py) = self.locate_anchor(file, *x, *y)?;
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
            Action::Wheel { dx, dy } => self.send(EventType::Wheel { delta_x: *dx, delta_y: *dy }),
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
                        anyhow::bail!("Detenido por el usuario.");
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
                        None => self.bot.log(format!("Carácter no soportado al escribir: {c:?}")),
                    }
                }
            }
            Action::RunCommand { command } => {
                self.bot.log(format!("Ejecutando: {command}"));
                #[cfg(windows)]
                let r = std::process::Command::new("cmd").args(["/C", command]).spawn();
                #[cfg(not(windows))]
                let r = std::process::Command::new("sh").args(["-c", command]).spawn();
                match r {
                    Ok(mut child) => drop(thread::spawn(move || child.wait())),
                    Err(e) => self.bot.log(format!("Error al ejecutar '{command}': {e}")),
                }
            }
            Action::Wait { ms } => self.sleep(*ms)?,
        }
        Ok(())
    }

    /// Espera a que el ancla aparezca en pantalla y devuelve dónde clicar.
    fn locate_anchor(&mut self, file: &str, x: f64, y: f64) -> anyhow::Result<(f64, f64)> {
        let path = self.route.anchor_path(file);
        let tpl = self
            .anchors
            .entry(file.to_string())
            .or_insert_with(|| image::open(&path).ok().map(|i| i.to_rgba8()))
            .clone();
        let Some(tpl) = tpl else { return Ok((x, y)) }; // sin ancla guardada
        let deadline = Instant::now() + Duration::from_millis(self.cfg.anchor_timeout_ms);
        let mut best = 0.0f32;
        loop {
            match vision::find_near(&tpl, x, y, self.cfg.search_radius) {
                Ok(m) if m.score >= self.cfg.threshold => {
                    if (m.x - x).abs() > 3.0 || (m.y - y).abs() > 3.0 {
                        self.bot.log(format!("Ancla desplazada: ({x:.0},{y:.0}) → ({:.0},{:.0}) [{:.2}]", m.x, m.y, m.score));
                    }
                    return Ok((m.x, m.y));
                }
                Ok(m) => best = best.max(m.score),
                Err(e) => {
                    self.bot.log(format!("Captura de pantalla falló ({e}); clic sin verificar."));
                    return Ok((x, y));
                }
            }
            if Instant::now() >= deadline {
                return match self.cfg.on_missing {
                    MissingAnchor::ClickAnyway => {
                        self.bot.log(format!("Ancla {file} no encontrada (mejor {best:.2}); clic en posición grabada."));
                        Ok((x, y))
                    }
                    MissingAnchor::Stop => anyhow::bail!("Ancla {file} no apareció en pantalla (mejor {best:.2}). Ruta detenida."),
                };
            }
            if self.bot.stopped() {
                anyhow::bail!("Detenido por el usuario.");
            }
            thread::sleep(Duration::from_millis(150));
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

/// Mapeo carácter → tecla física (distribución US). Devuelve (tecla, necesita Shift).
pub fn char_to_key(c: char) -> Option<(Key, bool)> {
    use Key::*;
    const LETTERS: [Key; 26] = [
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR,
        KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
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
        Step { delay_ms: d, action: Action::MouseMove { x: 1.0, y: 1.0 } }
    }

    #[test]
    fn simplify_keeps_last_move_and_sums_delays() {
        let mut steps = vec![mv(10), mv(10), mv(10), Step { delay_ms: 5, action: Action::KeyDown { key: Key::KeyA } }];
        simplify_moves(&mut steps);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].delay_ms, 30);
        assert_eq!(steps[1].delay_ms, 5);
    }

    #[test]
    fn clean_drops_orphan_release() {
        let mut steps = vec![
            Step { delay_ms: 0, action: Action::ButtonUp { button: Button::Left } },
            Step { delay_ms: 0, action: Action::KeyDown { key: Key::KeyW } },
            Step { delay_ms: 0, action: Action::KeyUp { key: Key::KeyW } },
        ];
        clean_recording(&mut steps, false);
        assert_eq!(steps.len(), 2);
    }

    #[test]
    fn clean_from_gui_drops_stop_click() {
        let down = |b| Step { delay_ms: 0, action: Action::ButtonDown { button: b, x: 0.0, y: 0.0, anchor: None } };
        let up = |b| Step { delay_ms: 0, action: Action::ButtonUp { button: b } };
        let mut steps = vec![down(Button::Right), up(Button::Right), mv(5), down(Button::Left), mv(1), up(Button::Left)];
        clean_recording(&mut steps, true);
        assert_eq!(steps.len(), 2);
    }

    #[test]
    fn chars_map() {
        assert_eq!(char_to_key('A'), Some((Key::KeyA, true)));
        assert_eq!(char_to_key('/'), Some((Key::Slash, false)));
        assert_eq!(char_to_key('!'), Some((Key::Num1, true)));
    }
}
