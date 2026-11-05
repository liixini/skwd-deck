use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use wall_proto::drawing::{Device, arg};

const DEPTH_MAP: &str = "--depth-map";

#[derive(Debug, Default, PartialEq)]
pub struct Arguments {
    pub pack: Option<PathBuf>,
    pub check: bool,
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub source_hash: Option<String>,
    pub depth_map: Option<PathBuf>,
    pub device: Option<Device>,
}

pub fn parse(words: impl IntoIterator<Item = OsString>) -> Result<Arguments> {
    let mut parsed = Arguments::default();
    let mut words = words.into_iter();
    while let Some(word) = words.next() {
        let mut value =
            || words.next().with_context(|| format!("{} needs a value", word.display()));
        match word.to_str() {
            Some(arg::PACK) => parsed.pack = Some(value()?.into()),
            Some(arg::INPUT) => parsed.input = Some(value()?.into()),
            Some(arg::OUTPUT) => parsed.output = Some(value()?.into()),
            Some(arg::SOURCE_HASH) => {
                parsed.source_hash = Some(value()?.to_string_lossy().into_owned());
            }
            Some(DEPTH_MAP) => parsed.depth_map = Some(value()?.into()),
            Some(arg::DEVICE) => {
                let label = value()?;
                parsed.device = Some(
                    label
                        .to_str()
                        .and_then(Device::parse)
                        .with_context(|| format!("unknown device {}", label.display()))?,
                );
            }
            Some(arg::CHECK) => parsed.check = true,
            _ => bail!("unknown argument {}", word.display()),
        }
    }
    Ok(parsed)
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
