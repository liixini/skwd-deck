#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_possible_wrap)]

mod builtin;
mod catalog;
mod render;
mod storage;
mod theme;

pub(crate) use catalog::list;
pub(crate) use render::render_effects_to_file;

#[cfg(test)]
use render::render_to_file;

#[cfg(test)]
use std::path::Path;

#[cfg(test)]
use builtin::{apply_border, apply_contrast, apply_gamma, apply_pixelate, apply_round};
#[cfg(test)]
use image::{DynamicImage, ImageReader, Rgba, RgbaImage};
#[cfg(test)]
use render::render_sync;
#[cfg(test)]
use serde_json::{Value, json};
#[cfg(test)]
use storage::{has_alpha, target_ext, webp_is_lossless};

#[cfg(test)]
mod tests;
