#![deny(unsafe_code)]

mod color;
pub mod gowall;
mod gowall_json;
#[cfg(feature = "material")]
pub mod material;
mod presets;
mod quantize;
pub mod seed;
pub mod semantic;
pub mod studio;
mod template_terminal;
mod theme;
mod theme_json;

pub use color::{Rgb, from_hsl, parse_hex, rotate, to_hsl};
pub use presets::{PRESETS, family, is_light_twin, light_twin, preset, relatives};
pub use quantize::quantize;
pub use template_terminal::{TEMPLATE_HUES, TemplateTerminal, template_terminal};
pub use theme::{ThemePalette, derive, generate};
