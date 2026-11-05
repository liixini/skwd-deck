use serde_json::{Map, Value, json};

use crate::{Rgb, ThemePalette, parse_hex};

pub const ANSI_SLOTS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Base,
    Panel,
    Raised,
    Text,
    Quiet,
    Accent,
    OnAccent,
    Second,
    Third,
}

impl Role {
    pub const ALL: [Self; 9] = [
        Self::Base,
        Self::Panel,
        Self::Raised,
        Self::Text,
        Self::Quiet,
        Self::Accent,
        Self::OnAccent,
        Self::Second,
        Self::Third,
    ];
    pub const NEUTRALS: [Self; 5] =
        [Self::Base, Self::Panel, Self::Raised, Self::Text, Self::Quiet];
    pub const ACCENTS: [Self; 3] = [Self::Accent, Self::Second, Self::Third];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Panel => "panel",
            Self::Raised => "raised",
            Self::Text => "text",
            Self::Quiet => "quiet",
            Self::Accent => "accent",
            Self::OnAccent => "onAccent",
            Self::Second => "second",
            Self::Third => "third",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|role| *role == self).unwrap_or(0)
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.key() == key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StudioPalette {
    pub base: Rgb,
    pub panel: Rgb,
    pub raised: Rgb,
    pub text: Rgb,
    pub quiet: Rgb,
    pub accent: Rgb,
    pub on_accent: Rgb,
    pub second: Rgb,
    pub third: Rgb,
    pub ansi: [Rgb; ANSI_SLOTS],
}

const ANSI_KEY: &str = "ansi";
const SCHEME_KEY: &str = "_scheme";
const COLORS_KEY: &str = "colors";
const COLOR_KEY: &str = "color";
const DARK_MODE: &str = "dark";
const LIGHT_MODE: &str = "light";
const MODES: [&str; 2] = [DARK_MODE, LIGHT_MODE];
const SECONDARY_ROLE: &str = "secondary";
pub(crate) const TERMINAL_KEYS: [(&str, usize); 12] = [
    ("ansi_red", 1),
    ("ansi_green", 2),
    ("ansi_yellow", 3),
    ("ansi_blue", 4),
    ("ansi_magenta", 5),
    ("ansi_cyan", 6),
    ("ansi_red_bright", 9),
    ("ansi_green_bright", 10),
    ("ansi_yellow_bright", 11),
    ("ansi_blue_bright", 12),
    ("ansi_magenta_bright", 13),
    ("ansi_cyan_bright", 14),
];

pub(crate) const fn mode_key(dark: bool) -> &'static str {
    if dark { DARK_MODE } else { LIGHT_MODE }
}

impl StudioPalette {
    #[must_use]
    pub fn mix(&self, to: &Self, frac: f32) -> Self {
        let mut ansi = self.ansi;
        for (slot, target) in ansi.iter_mut().zip(to.ansi) {
            *slot = slot.lerp(target, frac);
        }
        Self {
            base: self.base.lerp(to.base, frac),
            panel: self.panel.lerp(to.panel, frac),
            raised: self.raised.lerp(to.raised, frac),
            text: self.text.lerp(to.text, frac),
            quiet: self.quiet.lerp(to.quiet, frac),
            accent: self.accent.lerp(to.accent, frac),
            on_accent: self.on_accent.lerp(to.on_accent, frac),
            second: self.second.lerp(to.second, frac),
            third: self.third.lerp(to.third, frac),
            ansi,
        }
    }

    pub fn is_dark(&self) -> bool {
        super::oklch::to_oklch(self.base).light < super::polish::DARK_SURFACE_LIGHT
    }

    pub fn get(&self, role: Role) -> Rgb {
        match role {
            Role::Base => self.base,
            Role::Panel => self.panel,
            Role::Raised => self.raised,
            Role::Text => self.text,
            Role::Quiet => self.quiet,
            Role::Accent => self.accent,
            Role::OnAccent => self.on_accent,
            Role::Second => self.second,
            Role::Third => self.third,
        }
    }

    pub fn set(&mut self, role: Role, color: Rgb) {
        let slot = match role {
            Role::Base => &mut self.base,
            Role::Panel => &mut self.panel,
            Role::Raised => &mut self.raised,
            Role::Text => &mut self.text,
            Role::Quiet => &mut self.quiet,
            Role::Accent => &mut self.accent,
            Role::OnAccent => &mut self.on_accent,
            Role::Second => &mut self.second,
            Role::Third => &mut self.third,
        };
        *slot = color;
    }

    pub fn to_wire(&self) -> Value {
        let mut map: Map<String, Value> = Role::ALL
            .iter()
            .map(|role| (role.key().to_string(), json!(self.get(*role).hex())))
            .collect();
        map.insert(ANSI_KEY.to_string(), json!(self.ansi.map(Rgb::hex)));
        Value::Object(map)
    }

    pub fn from_wire(value: &Value) -> Option<Self> {
        let role = |role: Role| value.get(role.key()).and_then(Value::as_str).and_then(parse_hex);
        let ansi = value.get(ANSI_KEY)?.as_array().filter(|ansi| ansi.len() == ANSI_SLOTS)?;
        let mut slots = [Rgb(0, 0, 0); ANSI_SLOTS];
        for (slot, entry) in slots.iter_mut().zip(ansi) {
            *slot = entry.as_str().and_then(parse_hex)?;
        }
        Some(Self {
            base: role(Role::Base)?,
            panel: role(Role::Panel)?,
            raised: role(Role::Raised)?,
            text: role(Role::Text)?,
            quiet: role(Role::Quiet)?,
            accent: role(Role::Accent)?,
            on_accent: role(Role::OnAccent)?,
            second: role(Role::Second)?,
            third: role(Role::Third)?,
            ansi: slots,
        })
    }

    pub fn theme_value(&self) -> Value {
        let mut value = self.desktop_value();
        for (key, slot) in TERMINAL_KEYS {
            value[key] = json!(self.ansi[slot].hex());
        }
        value
    }

    pub fn desktop_value(&self) -> Value {
        let both = |color: Rgb| {
            let node: Map<String, Value> = MODES
                .iter()
                .map(|mode| ((*mode).to_string(), json!({ COLOR_KEY: color.hex() })))
                .collect();
            Value::Object(node)
        };
        let mut value = ThemePalette {
            primary: self.accent,
            on_primary: self.on_accent,
            surface: self.base,
            on_surface: self.text,
            surface_variant: self.raised,
            surface_container: self.panel,
            background: self.base,
            outline: self.quiet,
            tertiary: self.third,
        }
        .to_value();
        value[SCHEME_KEY] = json!({ COLORS_KEY: { SECONDARY_ROLE: both(self.second) } });
        value
    }
}

#[cfg(test)]
#[path = "palette_tests.rs"]
mod tests;
