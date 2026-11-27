//! Idiomas. El texto en español del código es la clave; las traducciones están en
//! `i18n.txt` (incluido en el ejecutable). `tr("…")` y `trf("… {} …", &[…])`.
//!
//! No se incluyen árabe ni hindi: egui no hace «shaping» ni escritura de derecha a
//! izquierda, y esos alfabetos se verían con las letras sueltas o al revés.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

/// (código, nombre nativo). El orden de las columnas de `i18n.txt` es el de este
/// array sin el español (la clave).
pub const IDIOMAS: [(&str, &str); 10] = [
    ("es", "Español"),
    ("en", "English"),
    ("zh", "中文 (Chinese)"),
    ("fr", "Français"),
    ("pt", "Português"),
    ("ru", "Русский"),
    ("de", "Deutsch"),
    ("ja", "日本語 (Japanese)"),
    ("ko", "한국어 (Korean)"),
    ("it", "Italiano"),
];

static IDIOMA: AtomicU8 = AtomicU8::new(0);
const TABLA: &str = include_str!("i18n.txt");

pub fn actual() -> usize {
    IDIOMA.load(Ordering::Relaxed) as usize
}

pub fn codigo() -> &'static str {
    IDIOMAS[actual()].0
}

/// Cambia el idioma por código ("en", "zh"…); los desconocidos se ignoran.
pub fn set(code: &str) {
    if let Some(i) = IDIOMAS.iter().position(|(c, _)| *c == code) {
        IDIOMA.store(i as u8, Ordering::Relaxed);
    }
}

/// Idioma del sistema operativo si está disponible; si no, inglés.
pub fn del_sistema() -> &'static str {
    let loc = sys_locale::get_locale().unwrap_or_default().to_lowercase();
    IDIOMAS
        .iter()
        .map(|(c, _)| *c)
        .find(|c| loc.starts_with(c))
        .unwrap_or("en")
}

/// ¿Necesita una fuente CJK del sistema (chino, japonés, coreano)?
pub fn es_cjk(code: &str) -> bool {
    matches!(code, "zh" | "ja" | "ko")
}

fn tabla() -> &'static HashMap<String, Vec<String>> {
    static T: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
    T.get_or_init(|| parse(TABLA))
}

/// Formato de `i18n.txt`: `= clave` y debajo una línea `código texto` por idioma.
/// `\n` dentro de un texto es un salto de línea.
fn parse(src: &str) -> HashMap<String, Vec<String>> {
    let mut out = HashMap::new();
    let mut clave: Option<String> = None;
    let mut fila = vec![String::new(); IDIOMAS.len() - 1];
    let mut guardar = |clave: &mut Option<String>, fila: &mut Vec<String>| {
        if let Some(k) = clave.take() {
            out.insert(
                k,
                std::mem::replace(fila, vec![String::new(); IDIOMAS.len() - 1]),
            );
        }
    };
    for linea in src.lines() {
        if let Some(k) = linea.strip_prefix("= ") {
            guardar(&mut clave, &mut fila);
            clave = Some(k.replace("\\n", "\n"));
        } else if let Some((code, texto)) = linea.split_once(' ') {
            if let Some(i) = IDIOMAS.iter().skip(1).position(|(c, _)| *c == code) {
                fila[i] = texto.replace("\\n", "\n");
            }
        }
    }
    guardar(&mut clave, &mut fila);
    out
}

/// Traduce un texto fijo. Sin traducción, devuelve el español.
pub fn tr(es: &'static str) -> &'static str {
    let i = actual();
    if i == 0 {
        return es;
    }
    match tabla().get(es).map(|f| f[i - 1].as_str()) {
        Some(t) if !t.is_empty() => t,
        _ => es,
    }
}

/// Traduce y rellena los `{}` en orden.
pub fn trf(es: &'static str, args: &[&dyn Display]) -> String {
    let mut out = String::new();
    let mut partes = tr(es).split("{}");
    out.push_str(partes.next().unwrap_or_default());
    for (i, p) in partes.enumerate() {
        if let Some(a) = args.get(i) {
            out.push_str(&a.to_string());
        }
        out.push_str(p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Claves usadas en el código: `tr("…")` y `trf("…"`.
    fn claves_del_codigo() -> Vec<String> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out = vec![];
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.extension().is_some_and(|x| x == "rs") && !p.ends_with("i18n.rs") {
                let src = std::fs::read_to_string(&p).unwrap();
                for pat in ["tr(", "trf("] {
                    for (i, _) in src.match_indices(pat) {
                        // `tr(` suelto (no `attr(`…) y el texto puede ir en la línea siguiente.
                        let antes = src[..i].chars().next_back();
                        if antes.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                            continue;
                        }
                        let resto = src[i + pat.len()..].trim_start();
                        let Some(resto) = resto.strip_prefix('"') else {
                            continue;
                        };
                        let mut k = String::new();
                        let mut it = resto.chars();
                        while let Some(c) = it.next() {
                            match c {
                                '"' => break,
                                '\\' => match it.next() {
                                    Some('n') => k.push('\n'),
                                    Some(o) => k.push(o),
                                    None => {}
                                },
                                // Un salto de línea real dentro de la cadena: en Windows Git lo
                                // convierte en \r\n y la clave deja de coincidir. Hay que usar `\n`.
                                '\n' => panic!(
                                    "{}: salto de línea real en una cadena de tr(); usa \\n",
                                    p.display()
                                ),
                                _ => k.push(c),
                            }
                        }
                        out.push(k);
                    }
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    #[test]
    fn every_key_in_code_is_translated() {
        let t = tabla();
        let faltan: Vec<_> = claves_del_codigo()
            .into_iter()
            .filter(|k| !t.contains_key(k))
            .collect();
        assert!(faltan.is_empty(), "faltan en i18n.txt: {faltan:#?}");
    }

    #[test]
    fn every_entry_has_all_languages_and_placeholders() {
        for (k, fila) in tabla() {
            let n = k.matches("{}").count();
            for (i, t) in fila.iter().enumerate() {
                let code = IDIOMAS[i + 1].0;
                assert!(!t.is_empty(), "«{k}» sin traducción a {code}");
                assert_eq!(
                    t.matches("{}").count(),
                    n,
                    "«{k}» en {code}: huecos {{}} distintos"
                );
            }
        }
    }

    #[test]
    fn no_unused_entries() {
        let usadas = claves_del_codigo();
        let sobran: Vec<_> = tabla().keys().filter(|k| !usadas.contains(k)).collect();
        assert!(sobran.is_empty(), "sobran en i18n.txt: {sobran:#?}");
    }

    #[test]
    fn trf_fills_in_order() {
        set("es");
        assert_eq!(trf("{} · {} pasos", &[&"r", &3]), "r · 3 pasos");
    }
}
