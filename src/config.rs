//! Configuración de la app (`replica.json` junto al ejecutable, modo portable):
//! máscara visual, rutas programadas y rutas cuyos programas ya se permitieron.

use crate::model::Route;
use chrono::{DateTime, Local, NaiveTime, TimeDelta};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    /// Índice de la máscara (ver `theme::MASCARAS`).
    pub tema: u8,
    pub schedules: Vec<Schedule>,
    /// Huellas de rutas cuyos programas el usuario ya permitió (ver `trust_key`).
    pub trusted: Vec<u64>,
    /// Windows: cerrar la ventana sale del todo en vez de quedarse en la bandeja.
    pub no_tray: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum When {
    /// Cada día a esta hora local.
    Daily { hour: u32, minute: u32 },
    /// Cada N minutos desde que se activa.
    Every { minutes: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    pub route: String,
    pub when: When,
    pub enabled: bool,
    /// Última vez que tocaba ejecutarse (se haya ejecutado o saltado por estar ocupado).
    #[serde(default)]
    pub last: Option<DateTime<Local>>,
}

/// Margen para lanzar una ejecución diaria: si la app se abre más tarde, no se lanza.
const DAILY_WINDOW_MIN: i64 = 5;

impl Schedule {
    /// ¿Toca ejecutarla ahora?
    pub fn is_due(&self, now: DateTime<Local>) -> bool {
        if !self.enabled {
            return false;
        }
        match self.when {
            When::Every { minutes } => match self.last {
                Some(last) => now - last >= TimeDelta::minutes(minutes.max(1) as i64),
                None => false, // empieza a contar al activarla (ver `arm`)
            },
            When::Daily { hour, minute } => {
                let Some(t) = NaiveTime::from_hms_opt(hour, minute, 0) else {
                    return false;
                };
                let Some(today) = now
                    .date_naive()
                    .and_time(t)
                    .and_local_timezone(Local)
                    .earliest()
                else {
                    return false;
                };
                let in_window = now >= today && now - today < TimeDelta::minutes(DAILY_WINDOW_MIN);
                let not_yet_today = self.last.is_none_or(|l| l < today);
                in_window && not_yet_today
            }
        }
    }

    /// Al activar una programación periódica, empieza a contar desde ahora.
    pub fn arm(&mut self, now: DateTime<Local>) {
        if matches!(self.when, When::Every { .. }) && self.last.is_none() {
            self.last = Some(now);
        }
    }

    pub fn label(&self) -> String {
        match self.when {
            When::Daily { hour, minute } => format!("cada día a las {hour:02}:{minute:02}"),
            When::Every { minutes } => format!("cada {minutes} min"),
        }
    }
}

impl Config {
    fn path() -> PathBuf {
        crate::model::routes_dir().with_file_name("replica.json")
    }

    pub fn load() -> Config {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn is_trusted(&self, route: &Route) -> bool {
        route.commands().is_empty() || self.trusted.contains(&trust_key(route))
    }

    pub fn trust(&mut self, route: &Route) {
        let k = trust_key(route);
        if !self.trusted.contains(&k) {
            self.trusted.push(k);
        }
    }
}

/// Huella del nombre y los programas de una ruta: si alguien cambia los comandos
/// del archivo, la ruta vuelve a pedir permiso.
// shortcut: DefaultHasher puede cambiar entre versiones de Rust (pediría permiso otra vez); usar SHA si molesta.
pub fn trust_key(route: &Route) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    route.name.hash(&mut h);
    route.commands().hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Action, Step};
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 11, 27, h, m, 0).unwrap()
    }

    fn sched(when: When) -> Schedule {
        Schedule {
            route: "r".into(),
            when,
            enabled: true,
            last: None,
        }
    }

    #[test]
    fn daily_fires_once_inside_window() {
        let mut s = sched(When::Daily {
            hour: 9,
            minute: 30,
        });
        assert!(!s.is_due(at(9, 29)));
        assert!(s.is_due(at(9, 30)));
        assert!(s.is_due(at(9, 34)));
        assert!(!s.is_due(at(9, 36)), "fuera del margen: no se lanza tarde");
        s.last = Some(at(9, 30));
        assert!(!s.is_due(at(9, 31)), "ya se lanzó hoy");
    }

    #[test]
    fn every_counts_from_activation() {
        let mut s = sched(When::Every { minutes: 10 });
        assert!(!s.is_due(at(10, 0)));
        s.arm(at(10, 0));
        assert!(!s.is_due(at(10, 9)));
        assert!(s.is_due(at(10, 10)));
        s.enabled = false;
        assert!(!s.is_due(at(10, 20)));
    }

    #[test]
    fn trust_follows_commands() {
        let mut r = Route {
            name: "x".into(),
            ..Default::default()
        };
        let mut c = Config::default();
        assert!(c.is_trusted(&r), "sin programas no hace falta permiso");
        r.steps.push(Step {
            delay_ms: 0,
            action: Action::RunCommand {
                command: "a.exe".into(),
            },
        });
        assert!(!c.is_trusted(&r));
        c.trust(&r);
        assert!(c.is_trusted(&r));
        r.steps[0].action = Action::RunCommand {
            command: "otro.exe".into(),
        };
        assert!(
            !c.is_trusted(&r),
            "si cambian los comandos, vuelve a pedir permiso"
        );
    }
}
