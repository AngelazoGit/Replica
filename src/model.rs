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
    pub steps: Vec<Step>,
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
