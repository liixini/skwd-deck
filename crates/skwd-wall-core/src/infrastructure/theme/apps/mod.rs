mod catalogue;
mod documents;
mod files;
mod manager;
mod plasma;
mod reload;
mod structured;
mod waybar;

pub use manager::{apply, list, set_enabled};
pub(crate) use waybar::protects_output;

#[cfg(test)]
mod tests;
