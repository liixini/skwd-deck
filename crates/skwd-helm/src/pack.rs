use std::path::PathBuf;

use serde_json::Value;
use skwd_wall_core::pack::{self, Asset, Pack, Template};

use crate::config::Config;

const REPLACE_BRANCHES: &[&str] = &[
    "display",
    "paper",
    "transition",
    "matugen",
    "weRender",
    "effects",
    skwd_config::keys::selector::COMPONENTS,
];

pub struct ExportOpts {
    pub name: String,
    pub wallpaper: Option<String>,
}

pub fn asset_kind(key: &str) -> &'static str {
    if key.starts_with("video:") {
        wall_proto::kind::VIDEO
    } else if key.starts_with("we:") {
        wall_proto::kind::WE
    } else if key.starts_with("http://") || key.starts_with("https://") {
        "url"
    } else {
        wall_proto::kind::STATIC
    }
}

fn asset_for_key(key: &str) -> Asset {
    let kind = asset_kind(key).to_string();
    let url = (kind == "url").then(|| key.to_string());
    Asset { kind, key: key.to_string(), url, bundled: None }
}

fn branch(overlay: &Value, dotted: &str) -> Option<Value> {
    let mut cur = overlay;
    for seg in dotted.split('.') {
        cur = cur.get(seg)?;
    }
    Some(cur.clone())
}

fn collect_templates(cfg: &Config) -> Vec<Template> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    let sources = cfg
        .root()
        .get("theme")
        .and_then(|theme| theme.get("nativeTemplates"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .chain(cfg.root().get("integrations").and_then(Value::as_array).into_iter().flatten());
    for entry in sources {
        let Some(src) =
            entry.get("template").and_then(Value::as_str).filter(|text| !text.is_empty())
        else {
            continue;
        };
        let name = src.rsplit('/').next().unwrap_or(src).to_string();
        if !seen.insert(name.clone()) {
            continue;
        }
        let resolved = if let Some(rest) = src.strip_prefix("~/") {
            format!("{}/{rest}", std::env::var("HOME").unwrap_or_default())
        } else {
            src.to_string()
        };
        if let Ok(contents) = std::fs::read_to_string(&resolved) {
            out.push(Template { name, contents });
        }
    }
    out
}

pub fn bundle_asset(cfg: &Config, key: &str, assets_dir: &std::path::Path) -> (Asset, Vec<String>) {
    let mut asset = asset_for_key(key);
    let mut warnings = Vec::new();
    match asset.kind.as_str() {
        wall_proto::kind::WE => {
            warnings.push(format!(
                "refusing to bundle Steam Workshop item {key} (Workshop ToS); referencing only"
            ));
        }
        wall_proto::kind::STATIC | wall_proto::kind::VIDEO => {
            let rel = key.split_once(':').map_or(key, |(_, rest)| rest);
            let base = if asset.kind == wall_proto::kind::VIDEO {
                cfg.video_dir()
            } else {
                cfg.wallpaper_dir()
            };
            let src = PathBuf::from(base.trim_end_matches('/')).join(rel);
            if rel.contains("wallhaven") {
                warnings.push(format!(
                    "{key} looks like third-party Wallhaven content; bundling it may breach its license"
                ));
            }
            let file_name = rel.rsplit('/').next().unwrap_or(rel);
            let dest = assets_dir.join(file_name);
            if std::fs::create_dir_all(assets_dir).is_ok() && std::fs::copy(&src, &dest).is_ok() {
                asset.bundled = Some(format!("assets/{file_name}"));
            } else {
                warnings
                    .push(format!("could not read {} to bundle; referencing only", src.display()));
            }
        }
        _ => {}
    }
    (asset, warnings)
}

pub fn build_pack(cfg: &Config, created_by: &str, opts: &ExportOpts) -> Pack {
    let overlay = pack::build_overlay(cfg.root());
    let mut pack = Pack::new(&opts.name, created_by, overlay);
    pack.manifest.wallpaper.clone_from(&opts.wallpaper);

    let palette_path = PathBuf::from(cfg.cache_dir()).join("skwd-colors.json");
    if let Ok(text) = std::fs::read_to_string(&palette_path)
        && let Ok(val) = serde_json::from_str::<Value>(&text)
    {
        pack.palette = Some(val);
    }

    pack.templates = collect_templates(cfg);

    if let Some(key) = &opts.wallpaper {
        pack.manifest.assets.push(asset_for_key(key));
    }
    pack
}

#[derive(Debug, Default)]
pub struct ImportReport {
    pub applied: Vec<String>,
    pub skipped_hooks: usize,
}

pub fn plan_import(overlay: &Value, allow_hooks: bool) -> (Vec<(String, Value)>, usize) {
    let apply = pack::import_keys(overlay, allow_hooks);
    let hooks_total =
        pack::overlay_keys(overlay).iter().filter(|(key, _)| pack::is_hook_key(key)).count();
    let skipped = if allow_hooks { 0 } else { hooks_total };
    (apply, skipped)
}

pub fn apply_import(
    cfg: &mut Config,
    pack: &Pack,
    replace: bool,
    allow_hooks: bool,
) -> ImportReport {
    if replace {
        for key in REPLACE_BRANCHES {
            if branch(&pack.overlay, key).is_some() {
                cfg.remove_key(key);
            }
        }
    }
    let (apply, skipped_hooks) = plan_import(&pack.overlay, allow_hooks);
    let mut applied = Vec::with_capacity(apply.len());
    for (key, val) in apply {
        cfg.set_key(&key, val);
        applied.push(key);
    }
    ImportReport { applied, skipped_hooks }
}

mod tests;
