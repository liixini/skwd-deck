use std::path::Path;

use anyhow::{Result, ensure};
use image::imageops::FilterType;
use image::{GrayAlphaImage, LumaA, RgbImage};
use paper_control::DrawingStroke;

use crate::plan::Event;
use crate::plane::Plane;
use crate::strokes::{Kind, Stroke};

pub const TEXTURE: &str = "drawing.png";
pub const STROKES: &str = "strokes.bin";
const RGB: usize = 3;
const PREVIEW_EDGE: u32 = 320;
const INK: usize = 0;
const MATTE: usize = 1;
const DEPTH: usize = 2;

pub struct Drawing<'a> {
    pub strokes: &'a [Stroke],
    pub events: &'a [Event],
    pub farther: &'a Plane,
    pub matte: &'a Plane,
    pub ink: &'a [u8],
    pub ink_size: [usize; 2],
}

impl Drawing<'_> {
    fn planned(&self) -> Vec<DrawingStroke> {
        self.events
            .iter()
            .map(|event| {
                let Stroke { points, width, kind } = &self.strokes[event.stroke];
                DrawingStroke {
                    start: event.start,
                    end: event.end,
                    width: *width,
                    hatch: *kind == Kind::Hatch,
                    points: points.clone(),
                }
            })
            .collect()
    }

    pub fn write(&self, output: &Path) -> Result<()> {
        std::fs::write(output.join(STROKES), DrawingStroke::encode(&self.planned()))?;
        let [ink_width, ink_height] = self.ink_size;
        let depth = self.farther.resize(ink_width, ink_height, FilterType::Triangle).to_bytes();
        ensure!(
            [self.matte.width, self.matte.height] == self.ink_size,
            "outline size differs from ink"
        );
        let matte = self.matte.to_bytes();
        let mut texture = vec![0; ink_width * ink_height * RGB];
        for index in 0..ink_width * ink_height {
            texture[index * RGB + INK] = self.ink[index];
            texture[index * RGB + MATTE] = matte[index];
            texture[index * RGB + DEPTH] = depth[index];
        }
        RgbImage::from_raw(ink_width as u32, ink_height as u32, texture)
            .expect("texture size")
            .save(output.join(TEXTURE))?;
        let preview = GrayAlphaImage::from_fn(ink_width as u32, ink_height as u32, |x, y| {
            LumaA([0, self.ink[y as usize * ink_width + x as usize]])
        });
        let scale = f64::from(PREVIEW_EDGE) / ink_width.max(ink_height) as f64;
        let [width, height] = [ink_width, ink_height]
            .map(|side| ((side as f64 * scale.min(1.0)).round() as u32).max(1));
        image::imageops::resize(&preview, width, height, FilterType::Triangle)
            .save(output.join(skwd_wall_core::infrastructure::drawing::PREVIEW))?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "artifact_tests.rs"]
mod tests;
