use super::palette::StudioPalette;
use super::sources::from_theme;
use super::tune::{Tuning, tune};

pub const THEME_PREFIX: &str = "theme:";
pub const MATERIAL_PREFIX: &str = "material:";
pub const AREA_KEY: &str = "area";
pub const MATUGEN_SCHEMES: [&str; 10] = [
    "tonal-spot",
    "content",
    "fidelity",
    "expressive",
    "vibrant",
    "monochrome",
    "fruit-salad",
    "neutral",
    "rainbow",
    "smart",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Method(String),
    Theme(String),
    Material(String),
}

impl Source {
    pub fn parse(key: &str) -> Self {
        if let Some(name) = key.strip_prefix(THEME_PREFIX) {
            return Self::Theme(name.to_string());
        }
        if let Some(scheme) = key.strip_prefix(MATERIAL_PREFIX) {
            return Self::Material(scheme.to_string());
        }
        Self::Method(key.to_string())
    }

    pub fn key(&self) -> String {
        match self {
            Self::Method(key) => key.clone(),
            Self::Theme(name) => format!("{THEME_PREFIX}{name}"),
            Self::Material(scheme) => format!("{MATERIAL_PREFIX}{scheme}"),
        }
    }
}

pub trait Reading {
    fn method(&self, key: &str, dark: bool) -> Option<StudioPalette>;
    fn material(&self, scheme: &str, dark: bool) -> Option<StudioPalette>;
}

fn themed(name: &str, dark: bool) -> &str {
    crate::light_twin(name).filter(|_| !dark).unwrap_or(name)
}

fn made(source: &Source, reading: Option<&impl Reading>, dark: bool) -> Option<StudioPalette> {
    match source {
        Source::Theme(name) => crate::preset(themed(name, dark)).map(|preset| from_theme(&preset)),
        Source::Material(scheme) => reading?.material(scheme, dark),
        Source::Method(key) => reading?.method(key, dark),
    }
}

pub fn resolve(
    source: &Source,
    reading: Option<&impl Reading>,
    tuning: &Tuning,
    mode_dark: impl FnOnce() -> bool,
) -> Option<(StudioPalette, bool)> {
    let wanted = match source {
        Source::Theme(name) => crate::light_twin(name).is_some() && mode_dark(),
        Source::Method(_) | Source::Material(_) => mode_dark(),
    };
    let palette = made(source, reading, wanted)?;
    let dark = match source {
        Source::Theme(_) => palette.is_dark(),
        Source::Method(_) | Source::Material(_) => wanted,
    };
    Some((tune(&palette, tuning, dark), dark))
}

#[cfg(test)]
#[path = "choice_tests.rs"]
mod tests;
