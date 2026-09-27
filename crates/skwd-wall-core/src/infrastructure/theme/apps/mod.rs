mod catalogue;
mod customization;
mod documents;
mod files;
mod foot;
mod manager;
mod plasma;
mod plasma_settings;
mod reload;
mod structured;
mod waybar;

pub use manager::{apply, customize, list, set_enabled};
pub(crate) use waybar::protects_output;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod fish_tests;
