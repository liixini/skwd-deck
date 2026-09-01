#![deny(unsafe_code)]

mod color;
pub mod gowall;
mod gowall_json;
mod presets;
mod quantize;
pub mod seed;
pub mod semantic;
mod theme;
mod theme_json;

pub use color::{Rgb, from_hsl, parse_hex, rotate, to_hsl};
pub use presets::{PRESETS, preset};
pub use quantize::quantize;
pub use theme::{ThemePalette, derive, generate};
