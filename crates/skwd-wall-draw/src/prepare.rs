use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result, ensure};
use image::RgbImage;
use image::imageops::FilterType;
use paper_control::{DRAWING_VERSION, DrawingManifest, DrawingSource};
use skwd_wall_core::infrastructure::drawing::{self as cache, PackStamp};
use wall_proto::drawing::{Progress, stage};

use crate::artifact::{self, Drawing};
use crate::models::{Locator, Models};
use crate::plan;
use crate::plane::Plane;
use crate::raster::Point;
use crate::strokes::{self, STROKE};
use crate::subject::{self, Found};

const INK_SIDE: f32 = 2560.0;
const TIMING_SIDE: f32 = 1920.0;

pub struct Step {
    stage: &'static str,
    progress: f32,
}

pub const SOURCE: Step = Step { stage: stage::SOURCE, progress: 0.05 };
pub const LINEART: Step = Step { stage: stage::LINEART, progress: 0.28 };
pub const DEPTH: Step = Step { stage: stage::DEPTH, progress: 0.49 };
pub const SUBJECTS: Step = Step { stage: stage::SUBJECTS, progress: 0.64 };
pub const COMPLETE: Step = Step { stage: stage::COMPLETE, progress: 1.0 };
pub const READY: Step = Step { stage: stage::READY, progress: 1.0 };

pub fn emit(step: &Step) {
    let progress = Progress { stage: step.stage.into(), progress: step.progress };
    println!("{}", serde_json::to_string(&progress).expect("progress line"));
}

pub struct Job<'args> {
    pub input: &'args Path,
    pub output: &'args Path,
    pub source_hash: &'args str,
}

pub fn vacant(output: &Path) -> bool {
    !output.exists() || std::fs::read_dir(output).is_ok_and(|mut entries| entries.next().is_none())
}

pub fn clear(output: &Path) -> Result<()> {
    if !output.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(output)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(entry.path())?;
        } else {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn fit(size: [usize; 2], side: f32) -> [usize; 2] {
    let scale = (side / size[0].max(size[1]) as f32).min(1.0);
    size.map(|length| ((length as f32 * scale).round() as usize).max(1))
}

pub fn ink_layers(strength: &Plane, timing: [usize; 2], cutoff: f32) -> (Vec<u8>, Plane) {
    let native = strength.ink(cutoff);
    let occupied = Plane {
        width: strength.width,
        height: strength.height,
        data: native.iter().map(|&value| f32::from(u8::from(value > 0))).collect(),
    }
    .resize(timing[0], timing[1], FilterType::Triangle);
    let mut working = strength.resize(timing[0], timing[1], FilterType::Triangle);
    for (value, occupancy) in working.data.iter_mut().zip(&occupied.data) {
        if *occupancy > 0.0 {
            *value = value.max(cutoff);
        }
    }
    (native, working)
}

fn luma(image: &RgbImage) -> Plane {
    let gray = image::imageops::grayscale(image);
    let data = gray.pixels().map(|pixel| f32::from(pixel[0]) / f32::from(u8::MAX)).collect();
    Plane { width: gray.width() as usize, height: gray.height() as usize, data }
}

fn origin(source: &Path, sha256: &str) -> Result<DrawingSource> {
    let metadata = source.metadata()?;
    Ok(DrawingSource {
        path: source.to_path_buf(),
        size: metadata.len(),
        mtime_ns: metadata.modified()?.duration_since(UNIX_EPOCH)?.as_nanos(),
        sha256: sha256.into(),
    })
}

fn manifest(
    source: DrawingSource,
    ink_size: [usize; 2],
    timing: [usize; 2],
    focus: Point,
) -> DrawingManifest {
    DrawingManifest {
        version: DRAWING_VERSION,
        source,
        width: ink_size[0] as u32,
        height: ink_size[1] as u32,
        texture: artifact::TEXTURE.into(),
        strokes: artifact::STROKES.into(),
        timing_width: timing[0] as u32,
        timing_height: timing[1] as u32,
        focus: [focus[0] / timing[0] as f32, focus[1] / timing[1] as f32],
    }
}

pub fn depth_map(models: &Models, input: &Path, output: &Path) -> Result<()> {
    let image = image::ImageReader::open(input)
        .context("depth map input is unavailable")?
        .with_guessed_format()?
        .decode()?
        .into_rgb8();
    let (width, height) = image.dimensions();
    let farther = models.depth(&image, [width as usize, height as usize])?;
    let near = farther.map(|value| 1.0 - value);
    image::GrayImage::from_raw(width, height, near.to_bytes())
        .context("depth map size")?
        .save(output)?;
    Ok(())
}

pub fn run(models: &Models, job: &Job) -> Result<()> {
    let source = job.input.canonicalize().context("wallpaper is unavailable")?;
    ensure!(source.is_file(), "wallpaper must be a file");
    ensure!(vacant(job.output), "output must be an empty directory");
    let before = PackStamp::capture(&source)?;
    let recorded = origin(&source, job.source_hash)?;
    ensure!(cache::hash_file(&source)? == job.source_hash, "wallpaper source hash changed");
    let original = image::ImageReader::open(&source)?.with_guessed_format()?.decode()?.into_rgb8();
    let ink_size = fit([original.width() as usize, original.height() as usize], INK_SIDE);
    let timing = fit(ink_size, TIMING_SIDE);
    let image = if [original.width() as usize, original.height() as usize] == ink_size {
        original
    } else {
        image::imageops::resize(
            &original,
            ink_size[0] as u32,
            ink_size[1] as u32,
            FilterType::Lanczos3,
        )
    };
    emit(&SOURCE);
    let strength = models.lineart(&image)?;
    emit(&LINEART);
    let farther = models.depth(&image, timing)?;
    emit(&DEPTH);
    let found = Found {
        faces: models.locate(Locator::Faces, &image, timing)?,
        heads: models.locate(Locator::Heads, &image, timing)?,
        matte: subject::matte(models.character(&image, timing)?, || {
            models.salient(&image, timing)
        })?,
        detections: models.subjects(&image, timing)?,
    };
    emit(&SUBJECTS);
    let subject = subject::find(&found, &farther);
    let matte = subject::outline(&found, &luma(&image));
    let (native, working) = ink_layers(&strength, timing, STROKE.ink_cutoff);
    let strokes = strokes::extract(&working, &strokes::bands(&farther))?;
    let events = plan::plan(&strokes, &subject);
    ensure!(
        before.valid() && cache::hash_file(&source)? == job.source_hash,
        "wallpaper source changed during preparation"
    );
    std::fs::create_dir_all(job.output)?;
    Drawing {
        strokes: &strokes,
        events: &events,
        farther: &farther,
        matte: &matte,
        ink: &native,
        ink_size,
    }
    .write(job.output)?;
    let manifest = manifest(recorded, ink_size, timing, subject.start);
    std::fs::write(job.output.join(cache::MANIFEST), serde_json::to_vec(&manifest)?)?;
    emit(&COMPLETE);
    Ok(())
}

#[cfg(test)]
#[path = "prepare_tests.rs"]
mod tests;
