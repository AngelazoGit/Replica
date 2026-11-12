//! Modelo de datos: una "ruta" es una secuencia de pasos con marcas de tiempo.
//! Se guarda en `routes/<nombre>/route.json` junto a las anclas visuales (PNG).

use rdev::{Button, Key};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const ROUTES_DIR: &str = "routes";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Action {
    MouseMove { x: f64, y: f64 },
    /// Pulsación de botón. `anchor` = PNG con lo que había en pantalla alrededor
    /// del cursor al grabar: al reproducir, el bot lo busca para saber dónde (y cuándo) clicar.
    ButtonDown { button: Button, x: f64, y: f64, anchor: Option<String> },
    ButtonUp { button: Button },
    Wheel { dx: i64, dy: i64 },
    KeyDown { key: Key },
    KeyUp { key: Key },
    /// Escribe texto (comandos de chat del juego, `/home`, etc.).
    TypeText { text: String },
    /// Lanza un programa / comando del sistema.
    RunCommand { command: String },
    /// Pausa explícita.
    Wait { ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    /// Milisegundos desde el paso anterior.
    pub delay_ms: u64,
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Route {
    pub name: String,
    /// Ventana sobre la que se grabó. Si existe, al reproducir las coordenadas
    /// se reescalan a su posición y tamaño actuales (otra resolución, ventana movida…).
    #[serde(default)]
    pub target: Option<Target>,
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

impl Target {
    /// Escala (horizontal, vertical) de `self` (al grabar) a `now` (al reproducir).
    pub fn scale_to(&self, now: &Target) -> (f64, f64) {
        (now.w as f64 / self.w.max(1) as f64, now.h as f64 / self.h.max(1) as f64)
    }

    /// Lleva un punto grabado sobre `self` a la misma posición relativa en `now`.
    pub fn map_to(&self, now: &Target, x: f64, y: f64) -> (f64, f64) {
        let (sx, sy) = self.scale_to(now);
        (now.x as f64 + (x - self.x as f64) * sx, now.y as f64 + (y - self.y as f64) * sy)
    }

    pub fn label(&self) -> String {
        let title: String = self.title.chars().take(40).collect();
        format!("{} — {title} ({}×{})", self.app, self.w, self.h)
    }
}

impl Route {
    pub fn dir(name: &str) -> PathBuf {
        Path::new(ROUTES_DIR).join(sanitize(name))
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let dir = Self::dir(&self.name);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("route.json"), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn load(name: &str) -> anyhow::Result<Route> {
        let data = std::fs::read_to_string(Self::dir(name).join("route.json"))?;
        Ok(serde_json::from_str(&data)?)
    }

    pub fn list() -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(ROUTES_DIR)
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

    pub fn duration_ms(&self) -> u64 {
        self.steps.iter().map(|s| s.delay_ms).sum()
    }
}

pub fn sanitize(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if s.is_empty() { "ruta".into() } else { s }
}

impl Action {
    pub fn describe(&self) -> String {
        match self {
            Action::MouseMove { x, y } => format!("Mover ratón a ({x:.0}, {y:.0})"),
            Action::ButtonDown { button, x, y, anchor } => format!(
                "Pulsar clic {button:?} en ({x:.0}, {y:.0}){}",
                if anchor.is_some() { "  [ancla visual]" } else { "" }
            ),
            Action::ButtonUp { button } => format!("Soltar clic {button:?}"),
            Action::Wheel { dx, dy } => format!("Rueda ({dx}, {dy})"),
            Action::KeyDown { key } => format!("Pulsar tecla {key:?}"),
            Action::KeyUp { key } => format!("Soltar tecla {key:?}"),
            Action::TypeText { text } => format!("Escribir \"{text}\""),
            Action::RunCommand { command } => format!("Ejecutar `{command}`"),
            Action::Wait { ms } => format!("Esperar {ms} ms"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_scales_and_moves_with_window() {
        let rec = Target { x: 100, y: 50, w: 1280, h: 720, ..Default::default() };
        let now = Target { x: 0, y: 0, w: 1920, h: 1080, ..Default::default() };
        // Centro de la ventana grabada → centro de la ventana actual.
        assert_eq!(rec.map_to(&now, 740.0, 410.0), (960.0, 540.0));
        assert_eq!(rec.map_to(&rec, 300.0, 200.0), (300.0, 200.0));
    }
}
