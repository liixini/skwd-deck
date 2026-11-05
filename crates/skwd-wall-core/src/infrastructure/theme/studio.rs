use std::collections::HashMap;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use serde_json::Value;
use skwd_config::keys::theme::MODE_DARK;
use skwd_palette::studio::{
    AREA_KEY, MATUGEN_SCHEMES, Method, Reading, Source, StudioPalette, Tuning, from_material,
    resolve,
};
use wall_proto::{StudioArea, StudioMethod, StudioResult};

use crate::config::Config;
use crate::lock;

const CACHE_CAP: usize = 64;
const MATUGEN: &str = "matugen";
const MATUGEN_ARGS: [&str; 4] = ["--dry-run", "-j", "hex", "--source-color-index"];
const MATUGEN_SOURCE_INDEX: &str = "0";
const MATUGEN_SCHEME_FLAG: &str = "-t";
const MATUGEN_MODE_FLAG: &str = "-m";
const MATUGEN_IMAGE: &str = "image";
const MATUGEN_SCHEME_PREFIX: &str = "scheme-";

type CacheKey = (String, Option<SystemTime>, Option<[u32; 4]>, bool);

fn area_arg(area: StudioArea) -> String {
    format!("{},{},{},{}", area.left, area.top, area.right, area.bottom)
}

fn cache_key(image: &str, area: Option<StudioArea>, schemes: bool) -> CacheKey {
    let modified = std::fs::metadata(image).and_then(|meta| meta.modified()).ok();
    let bits = area.map(|area| [area.left, area.top, area.right, area.bottom].map(f32::to_bits));
    (image.to_string(), modified, bits, schemes)
}

fn scan(image: &str, area: Option<StudioArea>) -> Option<StudioResult> {
    let mut cmd = Command::new(crate::paths::sibling_bin(crate::paths::SCAN_BIN));
    cmd.arg(wall_proto::STUDIO_SCAN_FLAG).arg(image);
    if let Some(area) = area {
        cmd.arg(wall_proto::STUDIO_AREA_FLAG).arg(area_arg(area));
    }
    let text = super::application::run_capture(&mut cmd)?;
    serde_json::from_str(&text)
        .map_err(|error| log::warn!("studio: unreadable scan output for {image}: {error}"))
        .ok()
}

fn matugen_document(image: &str, scheme: &str) -> Option<Value> {
    let mut cmd = Command::new(MATUGEN);
    cmd.arg(MATUGEN_IMAGE)
        .arg(image)
        .args(MATUGEN_ARGS)
        .arg(MATUGEN_SOURCE_INDEX)
        .arg(MATUGEN_SCHEME_FLAG)
        .arg(format!("{MATUGEN_SCHEME_PREFIX}{scheme}"))
        .arg(MATUGEN_MODE_FLAG)
        .arg(MODE_DARK);
    serde_json::from_str(&super::application::run_capture(&mut cmd)?).ok()
}

fn matugen(image: &str, scheme: &str, dark: bool) -> Option<StudioPalette> {
    from_material(&matugen_document(image, scheme)?, dark)
}

fn materials(image: &str) -> Vec<StudioMethod> {
    std::thread::scope(|scope| {
        let runs: Vec<_> = MATUGEN_SCHEMES
            .iter()
            .map(|scheme| scope.spawn(move || (scheme, matugen_document(image, scheme))))
            .collect();
        runs.into_iter()
            .filter_map(|run| run.join().ok())
            .filter_map(|(scheme, document)| {
                let document = document?;
                Some(StudioMethod {
                    key: (*scheme).to_string(),
                    dark: from_material(&document, true)?.to_wire(),
                    light: from_material(&document, false)?.to_wire(),
                    mark: None,
                    mask: Vec::new(),
                    sectors: Vec::new(),
                })
            })
            .collect()
    })
}

fn cache() -> &'static Mutex<HashMap<CacheKey, StudioResult>> {
    static CACHE: OnceLock<Mutex<HashMap<CacheKey, StudioResult>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn snapshot(image: &str, area: Option<StudioArea>, schemes: bool) -> Option<StudioResult> {
    let schemes = schemes && area.is_none();
    let key = cache_key(image, area, schemes);
    let rich = (key.0.clone(), key.1, key.2, true);
    if let Some(hit) = {
        let map = lock(cache());
        map.get(&key).or_else(|| map.get(&rich)).cloned()
    } {
        return Some(hit);
    }
    let plain = (key.0.clone(), key.1, key.2, false);
    let cached = schemes.then(|| lock(cache()).get(&plain).cloned()).flatten();
    let mut result = match cached {
        Some(found) => found,
        None => scan(image, area)?,
    };
    if schemes {
        result.materials = materials(image);
    }
    let mut map = lock(cache());
    if map.len() >= CACHE_CAP {
        map.clear();
    }
    map.insert(key, result.clone());
    Some(result)
}

pub fn tuning(config: &Config) -> Tuning {
    let theme = config.theme();
    Tuning::from_parts(
        theme.studio_warmth(),
        theme.studio_colour(),
        theme.studio_contrast(),
        theme.studio_shades(),
    )
}

struct Scan(StudioResult);

fn pick(entries: &[StudioMethod], key: &str, dark: bool) -> Option<StudioPalette> {
    let entry = entries.iter().find(|entry| entry.key == key)?;
    StudioPalette::from_wire(if dark { &entry.dark } else { &entry.light })
}

impl Reading for Scan {
    fn method(&self, key: &str, dark: bool) -> Option<StudioPalette> {
        pick(&self.0.methods, key, dark)
    }

    fn material(&self, scheme: &str, dark: bool) -> Option<StudioPalette> {
        pick(&self.0.materials, scheme, dark)
    }
}

pub fn mode_dark(config: &Config, image: &str) -> bool {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64);
    let local = skwd_log::local_time(seconds);
    let minute =
        (local.tm_hour * skwd_config::studio_time::MINUTES_PER_HOUR as i32 + local.tm_min) as u32;
    mode_dark_at(config, image, minute)
}

pub fn mode_dark_at(config: &Config, image: &str, minute: u32) -> bool {
    match skwd_config::studio_time::StudioMode::from_key(&config.theme().studio_mode()) {
        skwd_config::studio_time::StudioMode::Dark => true,
        skwd_config::studio_time::StudioMode::Light => false,
        skwd_config::studio_time::StudioMode::ByTime => skwd_config::studio_time::dark_at(minute),
        skwd_config::studio_time::StudioMode::FollowTheme => super::resolve_dark(config, image),
    }
}

struct Material<'a>(&'a str);

impl Reading for Material<'_> {
    fn method(&self, _key: &str, _dark: bool) -> Option<StudioPalette> {
        None
    }

    fn material(&self, scheme: &str, dark: bool) -> Option<StudioPalette> {
        let rich = cache_key(self.0, None, true);
        let cached = lock(cache()).get(&rich).and_then(|hit| pick(&hit.materials, scheme, dark));
        cached.or_else(|| matugen(self.0, scheme, dark))
    }
}

fn picked(config: &Config, image: &str) -> (Source, Option<StudioArea>) {
    let source = Source::parse(&config.theme().studio_method());
    if !matches!(&source, Source::Method(key) if key == AREA_KEY) {
        return (source, None);
    }
    let theme = config.theme();
    match theme.studio_area().filter(|_| theme.studio_area_image() == image) {
        Some(area) => (source, Some(area)),
        None => (Source::Method(Method::Tonal.key().to_string()), None),
    }
}

pub fn palette(config: &Config, image: &str) -> Option<(Value, bool)> {
    let (source, area) = picked(config, image);
    let tuning = tuning(config);
    let dark = || mode_dark(config, image);
    let (palette, dark) = match &source {
        Source::Theme(_) => resolve(&source, None::<&Scan>, &tuning, dark)?,
        Source::Material(_) => resolve(&source, Some(&Material(image)), &tuning, dark)?,
        Source::Method(_) => {
            resolve(&source, snapshot(image, area, false).map(Scan).as_ref(), &tuning, dark)?
        }
    };
    let value = match source {
        Source::Material(_) => palette.desktop_value(),
        Source::Method(_) | Source::Theme(_) => palette.theme_value(),
    };
    Some((value, dark))
}

#[cfg(test)]
#[path = "studio_tests.rs"]
mod tests;
