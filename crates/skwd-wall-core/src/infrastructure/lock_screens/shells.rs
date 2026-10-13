use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use skwd_config::keys::lock_screens as keys;
use toml_edit::{DocumentMut, Item, Table};

use super::ownership;

const DMS_SETTINGS: &str = "DankMaterialShell/settings.json";
const DMS_FIELD: &str = "lockScreenWallpaperPath";
const DMS_RECEIPT: &str = "dms.json";
const NOCTALIA_STATE_HOME: &str = "NOCTALIA_STATE_HOME";
const XDG_STATE_HOME: &str = "XDG_STATE_HOME";
const STATE_DIRECTORY: &str = ".local/state";
const NOCTALIA_SETTINGS: &str = "noctalia/settings.toml";
const NOCTALIA_RECEIPT: &str = "noctalia.json";
const CONFIG_VERSION: &str = "config_version";
const LOCKSCREEN: &str = "lockscreen";
const WALLPAPER: &str = "wallpaper";
const BLURRED_DESKTOP: &str = "blurred_desktop";
const FIRST_SUPPORTED_VERSION: i64 = 0;
const LAST_SUPPORTED_VERSION: i64 = 14;
const RECEIPT_VERSION: u32 = 1;

#[derive(Deserialize, Serialize)]
struct Receipt {
    version: u32,
    path: PathBuf,
    before: Option<String>,
    after: String,
    pending: Option<String>,
    table_created: bool,
    capture_before: Option<bool>,
    capture_owned: bool,
    #[serde(default)]
    created: bool,
}

impl Receipt {
    fn new(path: &Path, before: Option<String>, after: String) -> Self {
        Self {
            version: RECEIPT_VERSION,
            path: path.to_path_buf(),
            before,
            pending: Some(after.clone()),
            after,
            table_created: false,
            capture_before: None,
            capture_owned: false,
            created: false,
        }
    }
}

enum FieldAction {
    Set(String),
    Remove,
    Changed,
}

fn receipt(path: &Path) -> Result<Option<Receipt>> {
    ownership::read(path)?
        .map(|text| {
            let receipt: Receipt = serde_json::from_str(&text)?;
            ensure!(receipt.version == RECEIPT_VERSION, "Unsupported shell lockscreen receipt");
            Ok(receipt)
        })
        .transpose()
}

fn save(path: &Path, receipt: &Receipt) -> Result<()> {
    ownership::write(path, &serde_json::to_string(receipt)?)
}

fn field(current: Option<&str>, record: &Receipt, desired: Option<&str>) -> Result<FieldAction> {
    if current != Some(record.after.as_str())
        && current != record.pending.as_deref()
        && !(record.pending.as_deref() == Some(record.after.as_str())
            && current == record.before.as_deref())
    {
        if desired.is_some() {
            anyhow::bail!("Lockscreen setting changed outside Skwd: {}", record.path.display());
        }
        return Ok(FieldAction::Changed);
    }
    Ok(match desired.map_or_else(|| record.before.clone(), |path| Some(path.to_owned())) {
        Some(path) => FieldAction::Set(path),
        None => FieldAction::Remove,
    })
}

fn dms(config: &Value, image: Option<&Path>, root: &Path) -> Result<()> {
    let path = dms_settings();
    dms_at(config, image, root, &path)
}

pub(super) fn dms_settings() -> PathBuf {
    super::config_home().join(DMS_SETTINGS)
}

pub(super) fn sync_dms(config: &Value, image: Option<&Path>, root: &Path) -> Result<()> {
    dms(config, image, root)
}

fn dms_at(config: &Value, image: Option<&Path>, root: &Path, path: &Path) -> Result<()> {
    let record_path = root.join(DMS_RECEIPT);
    let enabled = super::enabled(config) && skwd_config::bool_false_unless_true(config, keys::DMS);
    if enabled && image.is_none() {
        return Ok(());
    }
    let desired = image.filter(|_| enabled).map(|path| path.to_string_lossy().into_owned());
    let saved = receipt(&record_path)?;
    if desired.is_none() && saved.is_none() {
        return super::backup::end(&record_path);
    }
    let original_text = ownership::read(path)?;
    let created = original_text.is_none();
    let mut settings: Value = match original_text {
        Some(text) => serde_json::from_str(&text).context("Invalid DMS settings.json")?,
        None => Value::Object(serde_json::Map::default()),
    };
    let object = settings.as_object_mut().context("DMS settings.json must be an object")?;
    let current = object
        .get(DMS_FIELD)
        .map(|value| value.as_str().context("DMS lock image setting must be a string"))
        .transpose()?;
    let original = current.map(ToOwned::to_owned);
    let record = match saved {
        Some(record) => {
            ensure!(record.path == path, "DMS settings path changed");
            record
        }
        None => Receipt {
            created,
            ..Receipt::new(path, original, desired.clone().context("Missing DMS image")?)
        },
    };
    super::backup::prepare(path, &record_path)?;
    match field(current, &record, desired.as_deref())? {
        FieldAction::Set(path) => {
            object.insert(DMS_FIELD.to_owned(), Value::String(path));
        }
        FieldAction::Remove => {
            object.remove(DMS_FIELD);
        }
        FieldAction::Changed => {
            std::fs::remove_file(&record_path)?;
            super::backup::end(&record_path)?;
            return Ok(());
        }
    }
    if let Some(image_path) = desired.as_ref() {
        let updated = Receipt { pending: Some(image_path.clone()), ..record };
        save(&record_path, &updated)?;
        ownership::write(path, &serde_json::to_string_pretty(&settings)?)?;
        save(&record_path, &Receipt { after: image_path.clone(), pending: None, ..updated })?;
    } else {
        if record.created && object.is_empty() {
            std::fs::remove_file(path)?;
        } else {
            ownership::write(path, &serde_json::to_string_pretty(&settings)?)?;
        }
        std::fs::remove_file(&record_path)?;
        super::backup::end(&record_path)?;
    }
    Ok(())
}

fn state_home() -> PathBuf {
    std::env::var_os(NOCTALIA_STATE_HOME)
        .or_else(|| std::env::var_os(XDG_STATE_HOME))
        .map_or_else(|| PathBuf::from(skwd_config::home()).join(STATE_DIRECTORY), PathBuf::from)
}

fn set_toml(section: &mut Item, key: &str, mut next: toml_edit::Value) {
    if let Some(item) = section.get_mut(key) {
        if let Some(old) = item.as_value() {
            *next.decor_mut() = old.decor().clone();
        }
        *item = Item::Value(next);
    } else {
        section[key] = Item::Value(next);
    }
}

fn noctalia(config: &Value, image: Option<&Path>, root: &Path) -> Result<()> {
    let path = noctalia_settings();
    noctalia_at(config, image, root, &path)
}

pub(super) fn noctalia_settings() -> PathBuf {
    state_home().join(NOCTALIA_SETTINGS)
}

pub(super) fn sync_noctalia(config: &Value, image: Option<&Path>, root: &Path) -> Result<()> {
    noctalia(config, image, root)
}

#[expect(clippy::too_many_lines, reason = "Noctalia lockscreen field ownership transaction")]
fn noctalia_at(config: &Value, image: Option<&Path>, root: &Path, path: &Path) -> Result<()> {
    let record_path = root.join(NOCTALIA_RECEIPT);
    let enabled =
        super::enabled(config) && skwd_config::bool_false_unless_true(config, keys::NOCTALIA);
    if enabled && image.is_none() {
        return Ok(());
    }
    let desired = image.filter(|_| enabled).map(|path| path.to_string_lossy().into_owned());
    let saved = receipt(&record_path)?;
    if desired.is_none() && saved.is_none() {
        return super::backup::end(&record_path);
    }
    let original_text = ownership::read(path)?;
    let created = original_text.is_none();
    let text = original_text.unwrap_or_default();
    let mut settings: DocumentMut = text.parse().context("Invalid Noctalia settings.toml")?;
    ensure!(
        settings.get(CONFIG_VERSION).is_none_or(|version| version.as_integer().is_some_and(
            |version| { (FIRST_SUPPORTED_VERSION..=LAST_SUPPORTED_VERSION).contains(&version) }
        )),
        "Unsupported Noctalia settings schema"
    );
    let section = settings.get(LOCKSCREEN);
    ensure!(section.is_none_or(Item::is_table), "Invalid Noctalia lockscreen table");
    let capture = section
        .and_then(|item| item.get(BLURRED_DESKTOP))
        .map(|item| item.as_bool().context("Noctalia desktop capture setting must be a boolean"))
        .transpose()?;
    let current = section
        .and_then(|item| item.get(WALLPAPER))
        .map(|item| item.as_str().context("Noctalia lock image setting must be a string"))
        .transpose()?;
    let original = current.map(ToOwned::to_owned);
    let fresh = saved.is_none();
    let mut record = match saved {
        Some(record) => {
            ensure!(record.path == path, "Noctalia settings path changed");
            record
        }
        None => Receipt {
            table_created: section.is_none(),
            capture_before: capture,
            capture_owned: capture != Some(false),
            created,
            ..Receipt::new(path, original, desired.clone().context("Missing Noctalia image")?)
        },
    };
    let unapplied = record.pending.as_deref() == Some(record.after.as_str())
        && current == record.before.as_deref()
        && capture == record.capture_before;
    let next = field(current, &record, desired.as_deref())?;
    super::backup::prepare(path, &record_path)?;
    if settings.get(LOCKSCREEN).is_none() {
        settings[LOCKSCREEN] = Item::Table(Table::new());
    }
    match next {
        FieldAction::Set(path) => {
            set_toml(&mut settings[LOCKSCREEN], WALLPAPER, toml_edit::Value::from(path));
        }
        FieldAction::Remove => {
            settings[LOCKSCREEN]
                .as_table_mut()
                .context("Invalid Noctalia lockscreen table")?
                .remove(WALLPAPER);
        }
        FieldAction::Changed => {}
    }
    if record.capture_owned {
        if fresh || unapplied {
            set_toml(&mut settings[LOCKSCREEN], BLURRED_DESKTOP, toml_edit::Value::from(false));
        } else if capture != Some(false) {
            record.capture_owned = false;
        } else if desired.is_none() {
            if let Some(before) = record.capture_before {
                set_toml(
                    &mut settings[LOCKSCREEN],
                    BLURRED_DESKTOP,
                    toml_edit::Value::from(before),
                );
            } else {
                settings[LOCKSCREEN]
                    .as_table_mut()
                    .context("Invalid Noctalia lockscreen table")?
                    .remove(BLURRED_DESKTOP);
            }
        }
    }
    if desired.is_none()
        && record.table_created
        && settings[LOCKSCREEN].as_table().is_some_and(Table::is_empty)
    {
        settings.remove(LOCKSCREEN);
    }
    if let Some(image_path) = desired.as_ref() {
        let updated = Receipt { pending: Some(image_path.clone()), ..record };
        save(&record_path, &updated)?;
        ownership::write(path, &settings.to_string())?;
        save(&record_path, &Receipt { after: image_path.clone(), pending: None, ..updated })?;
    } else {
        if record.created && settings.is_empty() {
            std::fs::remove_file(path)?;
        } else {
            ownership::write(path, &settings.to_string())?;
        }
        std::fs::remove_file(&record_path)?;
        super::backup::end(&record_path)?;
    }
    Ok(())
}

pub(super) fn sync(config: &Value, image: Option<&Path>, root: &Path) -> Result<()> {
    let mut errors = Vec::new();
    if let Err(error) = dms(config, image, root) {
        errors.push(format!("DMS: {error:#}"));
    }
    if let Err(error) = noctalia(config, image, root) {
        errors.push(format!("Noctalia: {error:#}"));
    }
    ensure!(errors.is_empty(), "{}", errors.join(super::ERROR_SEPARATOR));
    Ok(())
}

#[cfg(test)]
#[path = "shells/tests.rs"]
mod tests;
