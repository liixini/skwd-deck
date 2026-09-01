use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const SCHEMA_VERSION: u32 = 1;

const PORTABLE_PREFIXES: &[&str] = &[
    "display.",
    "paper.",
    "transition.",
    "matugen.",
    "weRender.",
    "effects.",
    "components.wallpaperSelector.",
];

const PORTABLE_EXACT: &[&str] = &[
    skwd_config::keys::theme::POLICY,
    skwd_config::keys::theme::AUTHORITY,
    skwd_config::keys::theme::ENGINE,
    skwd_config::keys::theme::MODE,
    skwd_config::keys::theme::WALLUST_PALETTE,
];

const IMPORT_COMPATIBILITY_EXACT: &[&str] = &[skwd_config::keys::theme::BACKEND];

pub fn is_portable_key(path: &str) -> bool {
    PORTABLE_EXACT.contains(&path)
        || PORTABLE_PREFIXES.iter().any(|prefix| path.starts_with(prefix))
}

pub fn is_secret_key(path: &str) -> bool {
    let last = path.rsplit('.').next().unwrap_or(path);
    matches!(last, "apiKey" | "accessKey" | "token" | "username")
}

pub fn is_hook_key(path: &str) -> bool {
    if path == "integrations" || path.starts_with("integrations.") {
        return true;
    }
    let last = path.rsplit('.').next().unwrap_or(path);
    last == "reload"
}

pub fn is_machine_local_key(path: &str) -> bool {
    if path.starts_with("paths.") {
        return true;
    }
    if is_hook_key(path) {
        return true;
    }
    matches!(
        path,
        "monitor"
            | "cache"
            | "externalMatugenCommand"
            | "defaultMatugenConfig"
            | skwd_config::keys::theme::NATIVE_COLORS_PATH
    )
}

fn flatten_into(prefix: &str, val: &Value, out: &mut Vec<(String, Value)>) {
    match val {
        Value::Object(map) if !map.is_empty() => {
            for (key, node) in map {
                let path = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
                flatten_into(&path, node, out);
            }
        }
        _ => out.push((prefix.to_string(), val.clone())),
    }
}

fn flatten(val: &Value) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    flatten_into("", val, &mut out);
    out.sort_by(|lhs, rhs| lhs.0.cmp(&rhs.0));
    out
}

fn insert_key(root: &mut Value, path: &str, value: Value) {
    let parts: Vec<&str> = path.split('.').collect();
    if !root.is_object() {
        *root = Value::Object(Map::default());
    }
    let mut cur = root;
    for part in &parts[..parts.len() - 1] {
        if !cur.is_object() {
            *cur = Value::Object(Map::default());
        }
        cur = cur
            .as_object_mut()
            .unwrap()
            .entry((*part).to_string())
            .or_insert_with(|| Value::Object(Map::default()));
    }
    if !cur.is_object() {
        *cur = Value::Object(Map::default());
    }
    cur.as_object_mut().unwrap().insert(parts[parts.len() - 1].to_string(), value);
}

pub fn build_overlay(config_root: &Value) -> Value {
    let mut root = Value::Object(Map::default());
    for (path, val) in flatten(config_root) {
        if is_portable_key(&path) && !is_secret_key(&path) && !is_machine_local_key(&path) {
            insert_key(&mut root, &path, val);
        }
    }
    root
}

pub fn overlay_keys(overlay: &Value) -> Vec<(String, Value)> {
    flatten(overlay)
}

pub fn import_keys(overlay: &Value, allow_hooks: bool) -> Vec<(String, Value)> {
    flatten(overlay)
        .into_iter()
        .filter(|(path, _)| {
            if is_secret_key(path) {
                return false;
            }
            if is_hook_key(path) {
                return allow_hooks;
            }
            if is_machine_local_key(path) {
                return false;
            }
            is_portable_key(path) || IMPORT_COMPATIBILITY_EXACT.contains(&path.as_str())
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub kind: String,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundled: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub name: String,
    pub created_by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallpaper: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<Asset>,
}

impl Manifest {
    pub fn new(name: &str, created_by: &str) -> Self {
        Self {
            schema: SCHEMA_VERSION,
            name: name.to_string(),
            created_by: created_by.to_string(),
            wallpaper: None,
            assets: Vec::new(),
        }
    }

    pub fn check_schema(&self) -> Result<()> {
        if self.schema > SCHEMA_VERSION {
            bail!(
                "pack schema {} is newer than this build supports (max {})",
                self.schema,
                SCHEMA_VERSION
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub name: String,
    pub contents: String,
}

#[derive(Debug, Clone)]
pub struct Pack {
    pub manifest: Manifest,
    pub overlay: Value,
    pub palette: Option<Value>,
    pub templates: Vec<Template>,
    pub library_jsonl: String,
}

impl Pack {
    pub fn new(name: &str, created_by: &str, overlay: Value) -> Self {
        Self {
            manifest: Manifest::new(name, created_by),
            overlay,
            palette: None,
            templates: Vec::new(),
            library_jsonl: String::new(),
        }
    }
}

fn template_file_name(name: &str) -> String {
    name.chars().map(|ch| if ch == '/' || ch == '\\' || ch == '\0' { '_' } else { ch }).collect()
}

pub fn write_pack(dir: &Path, pack: &Pack) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("create pack dir {}", dir.display()))?;
    let manifest = serde_json::to_string_pretty(&pack.manifest)?;
    std::fs::write(dir.join("manifest.json"), format!("{manifest}\n"))?;
    let overlay = serde_json::to_string_pretty(&pack.overlay)?;
    std::fs::write(dir.join("config.overlay.json"), format!("{overlay}\n"))?;
    if let Some(palette) = &pack.palette {
        let text = serde_json::to_string_pretty(palette)?;
        std::fs::write(dir.join("palette.json"), format!("{text}\n"))?;
    }
    if !pack.templates.is_empty() {
        let tdir = dir.join("templates");
        std::fs::create_dir_all(&tdir)?;
        for tpl in &pack.templates {
            std::fs::write(tdir.join(template_file_name(&tpl.name)), &tpl.contents)?;
        }
    }
    if !pack.library_jsonl.is_empty() {
        std::fs::write(dir.join("library.jsonl"), &pack.library_jsonl)?;
    }
    Ok(())
}

fn read_json(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

pub fn read_pack(dir: &Path) -> Result<Pack> {
    let manifest_text = std::fs::read_to_string(dir.join("manifest.json"))
        .with_context(|| format!("read manifest in {}", dir.display()))?;
    let manifest: Manifest = serde_json::from_str(&manifest_text).context("parse manifest.json")?;
    manifest.check_schema()?;

    let overlay_path = dir.join("config.overlay.json");
    let overlay = if overlay_path.exists() {
        read_json(&overlay_path)?
    } else {
        Value::Object(Map::default())
    };

    let palette_path = dir.join("palette.json");
    let palette = if palette_path.exists() { Some(read_json(&palette_path)?) } else { None };

    let mut templates = Vec::new();
    let tdir = dir.join("templates");
    if tdir.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(&tdir)?
            .filter_map(std::result::Result::ok)
            .filter(|ent| ent.path().is_file())
            .collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for ent in entries {
            let name = ent.file_name().to_string_lossy().into_owned();
            let contents = std::fs::read_to_string(ent.path())?;
            templates.push(Template { name, contents });
        }
    }

    let lib_path = dir.join("library.jsonl");
    let library_jsonl =
        if lib_path.exists() { std::fs::read_to_string(&lib_path)? } else { String::new() };

    Ok(Pack { manifest, overlay, palette, templates, library_jsonl })
}

#[path = "tests.rs"]
mod tests;
