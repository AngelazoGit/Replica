//! Modelo de datos: una "ruta" es una secuencia de pasos con marcas de tiempo.
//! Se guarda en `routes/<nombre>/route.json` junto a las anclas visuales (PNG).

use crate::i18n::{tr, trf};
use rdev::{Button, Key};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Carpeta `routes` junto al ejecutable (modo portable: todo viaja con el .exe,
/// da igual desde dónde se abra).
pub fn routes_dir() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        exe_dir.unwrap_or_default().join("routes")
    })
    .clone()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Action {
    MouseMove {
        x: f64,
        y: f64,
    },
    /// Pulsación de botón. `anchor` = PNG con lo que había en pantalla alrededor
    /// del cursor al grabar: al reproducir, el bot lo busca para saber dónde (y cuándo) clicar.
    ButtonDown {
        button: Button,
        x: f64,
        y: f64,
        anchor: Option<String>,
    },
    ButtonUp {
        button: Button,
    },
    Wheel {
        dx: i64,
        dy: i64,
    },
    KeyDown {
        key: Key,
    },
    KeyUp {
        key: Key,
    },
    /// Escribe texto (comandos de chat del juego, `/home`, etc.).
    TypeText {
        text: String,
    },
    /// Lanza un programa / comando del sistema.
    RunCommand {
        command: String,
    },
    /// Pausa explícita.
    Wait {
        ms: u64,
    },
    /// Clic donde esté el ratón, sin moverlo (autoclicker).
    ClickHere {
        button: Button,
    },
    /// Ejecuta otra ruta guardada (una vuelta) y sigue con esta.
    RunRoute {
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// Milisegundos desde el paso anterior.
    pub delay_ms: u64,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Route {
    pub name: String,
    /// Ventana sobre la que se grabó. Si existe, al reproducir las coordenadas
    /// se reescalan a su posición y tamaño actuales (otra resolución, ventana movida…).
    #[serde(default)]
    pub target: Option<Target>,
    /// Ajustes con los que se guardó (None en rutas antiguas: se usan los actuales).
    #[serde(default)]
    pub settings: Option<Settings>,
    pub steps: Vec<Step>,
}

/// Ventana de una aplicación y su rectángulo en coordenadas globales.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Target {
    pub app: String,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MissingAnchor {
    /// Si el ancla no aparece a tiempo, clica en la posición grabada.
    ClickAnyway,
    /// Si el ancla no aparece a tiempo, detiene la ruta (más seguro).
    Stop,
}

/// Ajustes de reproducción. Se guardan con cada ruta (`route.json`), así cada ruta
/// recuerda su velocidad, vueltas, política visual, cuenta atrás y atajo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
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
    /// Segundos de cuenta atrás antes de reproducir (para cambiar a la ventana del juego).
    pub countdown_s: u32,
    /// Atajo global que lanza esta ruta directamente (F1–F8).
    pub hotkey: Option<Key>,
    /// Si un paso falla (ancla, ventana…), repetir la vuelta desde el principio hasta N veces.
    pub retries: u32,
    /// Al grabar, las esperas de más de N s se recortan a N s (0 = no recortar).
    pub trim_wait_s: u32,
    /// Parar las vueltas cuando aparezca (o desaparezca) la imagen `until.png` de la ruta.
    pub until: Option<Until>,
}

/// Condición de parada: la imagen `until.png` (capturada en `x, y`) aparece o desaparece.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Until {
    pub x: f64,
    pub y: f64,
    pub appear: bool,
}

/// Archivo de la imagen de la condición de parada, dentro de la carpeta de la ruta.
pub const UNTIL_FILE: &str = "until.png";

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
            countdown_s: 3,
            hotkey: None,
            retries: 0,
            trim_wait_s: 0,
            until: None,
        }
    }
}

impl Target {
    /// Escala (horizontal, vertical) de `self` (al grabar) a `now` (al reproducir).
    pub fn scale_to(&self, now: &Target) -> (f64, f64) {
        (
            now.w as f64 / self.w.max(1) as f64,
            now.h as f64 / self.h.max(1) as f64,
        )
    }

    /// Lleva un punto grabado sobre `self` a la misma posición relativa en `now`.
    pub fn map_to(&self, now: &Target, x: f64, y: f64) -> (f64, f64) {
        let (sx, sy) = self.scale_to(now);
        (
            now.x as f64 + (x - self.x as f64) * sx,
            now.y as f64 + (y - self.y as f64) * sy,
        )
    }

    pub fn label(&self) -> String {
        let title: String = self.title.chars().take(40).collect();
        format!("{} — {title} ({}×{})", self.app, self.w, self.h)
    }
}

impl Route {
    pub fn dir(name: &str) -> PathBuf {
        routes_dir().join(sanitize(name))
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let dir = Self::dir(&self.name);
        std::fs::create_dir_all(&dir)?;
        // Escribir aparte y renombrar: un cierre a mitad de escritura no corrompe la ruta.
        let tmp = dir.join("route.json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(tmp, dir.join("route.json"))?;
        Ok(())
    }

    pub fn load(name: &str) -> anyhow::Result<Route> {
        let data = std::fs::read_to_string(Self::dir(name).join("route.json"))?;
        Ok(serde_json::from_str(&data)?)
    }

    pub fn list() -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(routes_dir())
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().join("route.json").exists())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        out.sort();
        out
    }

    pub fn anchor_path(&self, file: &str) -> PathBuf {
        Self::dir(&self.name).join(file)
    }

    /// Programas que ejecuta la ruta (para pedir confirmación antes de reproducirla).
    pub fn commands(&self) -> Vec<String> {
        self.steps
            .iter()
            .filter_map(|s| match &s.action {
                Action::RunCommand { command } => Some(command.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn duration_ms(&self) -> u64 {
        self.steps.iter().map(|s| s.delay_ms).sum()
    }
}

pub fn sanitize(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() { "ruta".into() } else { s }
}

/// Recetas rápidas: rutas que no hace falta grabar. Se repiten hasta F12.
pub enum Receta {
    /// Clic donde esté el ratón cada `ms`.
    Autoclic { ms: u64, button: Button },
    /// Pulsa una tecla cada `min` minutos para no quedar inactivo.
    AntiAfk { min: u32, key: Key },
    /// Pulsa una tecla cada `ms`.
    Tecla { ms: u64, key: Key },
}

impl Receta {
    pub fn ruta(&self) -> (Route, Settings) {
        let paso = |delay_ms, action| Step { delay_ms, action };
        let toque = |key| {
            vec![
                paso(0, Action::KeyDown { key }),
                paso(60, Action::KeyUp { key }),
            ]
        };
        let (name, steps, pausa, humanize) = match *self {
            Receta::Autoclic { ms, button } => (
                tr("Autoclicker"),
                vec![paso(0, Action::ClickHere { button })],
                ms,
                false,
            ),
            Receta::AntiAfk { min, key } => (tr("Anti-AFK"), toque(key), min as u64 * 60_000, true),
            Receta::Tecla { ms, key } => (tr("Repetir tecla"), toque(key), ms, false),
        };
        let cfg = Settings {
            loops: 0,
            pause_between_loops_ms: pausa,
            humanize,
            use_anchors: false,
            ..Settings::default()
        };
        (
            Route {
                name: name.into(),
                steps,
                ..Default::default()
            },
            cfg,
        )
    }
}

/// Nombre del botón del ratón en el idioma actual.
fn boton(b: &Button) -> String {
    match b {
        Button::Left => tr("izquierdo").into(),
        Button::Right => tr("derecho").into(),
        Button::Middle => tr("central").into(),
        other => format!("{other:?}"),
    }
}

impl Action {
    pub fn describe(&self) -> String {
        match self {
            Action::MouseMove { x, y } => trf(
                "Mover ratón a ({}, {})",
                &[&format!("{x:.0}"), &format!("{y:.0}")],
            ),
            Action::ButtonDown {
                button,
                x,
                y,
                anchor,
            } => {
                let pos = format!("({x:.0}, {y:.0})");
                let t = trf("Pulsar clic {} en {}", &[&boton(button), &pos]);
                if anchor.is_some() {
                    format!("{t}  [{}]", tr("ancla visual"))
                } else {
                    t
                }
            }
            Action::ButtonUp { button } => trf("Soltar clic {}", &[&boton(button)]),
            Action::Wheel { dx, dy } => trf("Rueda ({}, {})", &[dx, dy]),
            Action::KeyDown { key } => trf("Pulsar tecla {}", &[&format!("{key:?}")]),
            Action::KeyUp { key } => trf("Soltar tecla {}", &[&format!("{key:?}")]),
            Action::TypeText { text } => trf("Escribir «{}»", &[text]),
            Action::RunCommand { command } => trf("Ejecutar `{}`", &[command]),
            Action::ClickHere { button } => trf("Clic {} donde esté el ratón", &[&boton(button)]),
            Action::RunRoute { name } => trf("Ejecutar la ruta «{}»", &[name]),
            Action::Wait { ms } => trf("Esperar {} ms", &[ms]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_routes_load() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/routes");
        let mut n = 0;
        for e in std::fs::read_dir(dir).unwrap() {
            let json = std::fs::read_to_string(e.unwrap().path().join("route.json")).unwrap();
            let r: Route = serde_json::from_str(&json).unwrap();
            assert!(!r.steps.is_empty());
            // Solo nota_rapida ejecuta programas (y por eso pide permiso al cargarla).
            assert_eq!(!r.commands().is_empty(), r.name == "nota_rapida");
            n += 1;
        }
        assert_eq!(n, 3);
    }

    #[test]
    fn recipes_loop_forever_without_anchors() {
        let (r, cfg) = Receta::AntiAfk {
            min: 5,
            key: Key::Space,
        }
        .ruta();
        assert_eq!(cfg.loops, 0, "hasta F12");
        assert_eq!(cfg.pause_between_loops_ms, 300_000);
        assert!(!cfg.use_anchors);
        assert_eq!(r.steps.len(), 2, "pulsar y soltar");
        let (r, cfg) = Receta::Autoclic {
            ms: 100,
            button: Button::Left,
        }
        .ruta();
        assert_eq!(
            r.steps[0].action,
            Action::ClickHere {
                button: Button::Left
            }
        );
        assert_eq!(cfg.pause_between_loops_ms, 100);
    }

    #[test]
    fn map_scales_and_moves_with_window() {
        let rec = Target {
            x: 100,
            y: 50,
            w: 1280,
            h: 720,
            ..Default::default()
        };
        let now = Target {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
            ..Default::default()
        };
        // Centro de la ventana grabada → centro de la ventana actual.
        assert_eq!(rec.map_to(&now, 740.0, 410.0), (960.0, 540.0));
        assert_eq!(rec.map_to(&rec, 300.0, 200.0), (300.0, 200.0));
    }
}
