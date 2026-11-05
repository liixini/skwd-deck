use std::collections::BTreeSet;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
pub use wall_proto::drawing::Device;

pub const VERSION: u32 = 3;
const RECORD_LIMIT: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub enum DepthModel {
    #[default]
    #[serde(rename = "depth-anything-v2")]
    AnythingV2,
    #[serde(rename = "depth-anything-3")]
    Anything3,
}

#[derive(Deserialize)]
struct Header {
    version: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    pub version: u32,
    pub runtime: PathBuf,
    pub device: Device,
    #[serde(default)]
    pub webgpu: Option<PathBuf>,
    pub lineart: PathBuf,
    #[serde(default)]
    pub lineart_cpu_nodes: Vec<String>,
    pub depth: PathBuf,
    #[serde(default)]
    pub depth_model: DepthModel,
    #[serde(default)]
    pub depth_side: Option<NonZeroU32>,
    pub subject: PathBuf,
    pub character: PathBuf,
    pub salient: PathBuf,
    pub faces: PathBuf,
    pub heads: PathBuf,
    pub subject_classes: BTreeSet<usize>,
    pub subject_confidence: f32,
}

impl Pack {
    pub fn read(path: &Path) -> Result<Self> {
        let path = path.canonicalize().context("drawing pack is unavailable")?;
        ensure!(
            std::fs::metadata(&path)?.len() <= RECORD_LIMIT,
            "drawing pack manifest is too large"
        );
        let bytes = std::fs::read(&path)?;
        let header: Header = serde_json::from_slice(&bytes)?;
        ensure!(header.version == VERSION, "unsupported drawing pack version");
        let mut pack: Self = serde_json::from_slice(&bytes)?;
        ensure!(!pack.subject_classes.is_empty(), "drawing pack names no subject classes");
        ensure!(
            (0.0..=1.0).contains(&pack.subject_confidence),
            "drawing pack subject confidence is out of range"
        );
        ensure!(
            pack.device != Device::Webgpu || pack.webgpu.is_some(),
            "drawing pack device webgpu needs a webgpu plugin"
        );
        let root = path.parent().context("drawing pack has no directory")?;
        for file in [
            &mut pack.runtime,
            &mut pack.lineart,
            &mut pack.depth,
            &mut pack.subject,
            &mut pack.character,
            &mut pack.salient,
            &mut pack.faces,
            &mut pack.heads,
        ]
        .into_iter()
        .chain(pack.webgpu.iter_mut())
        {
            *file = root.join(&*file);
            ensure!(file.is_file(), "drawing pack file {} is missing", file.display());
        }
        Ok(pack)
    }

    pub fn files(&self) -> impl Iterator<Item = &Path> {
        [
            &self.runtime,
            &self.lineart,
            &self.depth,
            &self.subject,
            &self.character,
            &self.salient,
            &self.faces,
            &self.heads,
        ]
        .into_iter()
        .chain(self.webgpu.iter())
        .map(PathBuf::as_path)
    }
}

#[cfg(test)]
#[path = "pack_tests.rs"]
mod tests;
