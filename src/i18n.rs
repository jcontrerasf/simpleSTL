//! Idioma de la interfaz: español o inglés. Se detecta del sistema al iniciar y se puede
//! cambiar desde la barra de estado (no se guarda entre sesiones).
//!
//! Los textos se escriben en el lugar donde se usan, con sus dos versiones: `tr` para los
//! fijos y `match i18n::current()` para las frases con parámetros, donde el orden de las
//! palabras cambia entre idiomas.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Es,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Es, Lang::En];

    /// Nombre del idioma en ese mismo idioma, para el selector.
    pub fn name(self) -> &'static str {
        match self {
            Lang::Es => "Español",
            Lang::En => "English",
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(Lang::Es as u8);

pub fn current() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == Lang::En as u8 { Lang::En } else { Lang::Es }
}

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

/// Español si el sistema está en español (cualquier variante: "es-CL", "es_ES.UTF-8", …);
/// inglés en cualquier otro caso.
pub fn detect() -> Lang {
    match sys_locale::get_locale() {
        Some(locale) if locale.to_lowercase().starts_with("es") => Lang::Es,
        _ => Lang::En,
    }
}

/// El texto en el idioma activo.
pub fn tr(es: &'static str, en: &'static str) -> &'static str {
    match current() {
        Lang::Es => es,
        Lang::En => en,
    }
}
