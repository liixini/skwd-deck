use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use skwd_config::keys::{semantic, weather};
use skwd_wall_core::{WallState, db};

use super::{ALL_OUTPUTS, ITEM_PATH, ITEM_TYPE, ITEM_WE_ID, current_keys};
use wall_rules::outdoors::Context;

const SAVED_KEYS: [&str; 3] = [weather::SOURCE, weather::ENABLED, semantic::ENABLED];

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct Session {
    pub active: bool,
    pub starting: bool,
    pub settings: Vec<(String, Value)>,
    pub applied_settings: Vec<(String, Value)>,
    pub assignments: Vec<(String, i64)>,
    pub wallpapers: BTreeMap<String, String>,
    pub owned: BTreeMap<String, String>,
    pub contexts: BTreeMap<String, Context>,
    pub location_key: Option<String>,
    pub held: BTreeSet<String>,
    pub excluded: BTreeSet<String>,
}

impl Session {
    pub fn capture(state: &Arc<WallState>) -> anyhow::Result<Self> {
        let config = state.config().clone();
        Ok(Self {
            active: true,
            starting: true,
            settings: SAVED_KEYS
                .iter()
                .map(|key| {
                    (
                        (*key).to_owned(),
                        skwd_config::get(config.root(), key).cloned().unwrap_or(Value::Null),
                    )
                })
                .collect(),
            applied_settings: enabled_settings(state),
            assignments: state.with_db(db::playlist_assigns)?,
            wallpapers: current_keys(state),
            ..Self::default()
        })
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        skwd_config::atomic_write(path, &serde_json::to_vec(self)?)?;
        Ok(())
    }

    pub fn recover(&mut self, state: &Arc<WallState>, path: &Path) -> anyhow::Result<()> {
        if !self.starting {
            return Ok(());
        }
        if super::active(&state.config()) {
            state.with_db(|connection| {
                let transaction = connection.unchecked_transaction()?;
                for (output, id) in db::playlist_assigns(&transaction)? {
                    if self.assignments.contains(&(output.clone(), id)) {
                        db::playlist_assign_set(&transaction, &output, None)?;
                    }
                }
                transaction.commit()
            })?;
        } else {
            self.active = false;
        }
        self.starting = false;
        self.save(path)
    }

    pub fn release_empty(&self, ctx: &crate::composition::context::Ctx) -> anyhow::Result<()> {
        let current = current_keys(&ctx.state);
        let owned: Vec<_> = self
            .owned
            .iter()
            .filter(|(output, key)| {
                !self.wallpapers.contains_key(*output)
                    && !self.held.contains(*output)
                    && !self.held.contains(ALL_OUTPUTS)
                    && current.get(*output) == Some(*key)
            })
            .collect();
        if owned.is_empty() {
            return Ok(());
        }
        let outputs: Vec<_> = owned.iter().map(|(output, _)| (*output).clone()).collect();
        let cache = ctx.state.config().cache_dir();
        let recorded = skwd_wall_core::audio::read_state(&cache);
        ctx.wallpaper.release_outputs(&outputs)?;
        let remaining = current_keys(&ctx.state);
        for (output, key) in owned {
            if remaining.values().any(|current| current == key) {
                continue;
            }
            let Some(record) = recorded.get(output).or_else(|| recorded.get(ALL_OUTPUTS)) else {
                continue;
            };
            let (path, we_id) =
                if record.get(ITEM_TYPE).and_then(Value::as_str) == Some(wall_proto::kind::WE) {
                    ("", record.get(ITEM_WE_ID).and_then(Value::as_str).unwrap_or_default())
                } else {
                    (record.get(ITEM_PATH).and_then(Value::as_str).unwrap_or_default(), "")
                };
            crate::infrastructure::persistence::forget_last_wallpaper_if(path, we_id);
            crate::infrastructure::restore_policy::forget_wallpaper(&cache, path, we_id);
        }
        Ok(())
    }

    pub fn restore_settings(&self, state: &Arc<WallState>) -> anyhow::Result<()> {
        state.reload_config();
        let current = state.config().clone();
        let owned = &self.applied_settings;
        let settings: Vec<_> = self
            .settings
            .iter()
            .filter(|(key, _)| {
                owned.iter().any(|(name, value)| {
                    name == key && skwd_config::get(current.root(), key) == Some(value)
                })
            })
            .cloned()
            .collect();
        save_config(state, &settings)
    }

    pub fn restore_assignments(&self, state: &Arc<WallState>) -> anyhow::Result<()> {
        state.with_db(|connection| {
            let transaction = connection.unchecked_transaction()?;
            let current = db::playlist_assigns(&transaction)?;
            let playlists = db::playlists_all(&transaction)?;
            for (output, id) in &self.assignments {
                if !playlists.iter().any(|playlist| playlist.id == *id) {
                    continue;
                }
                let outputs = if output == ALL_OUTPUTS && !self.held.is_empty() {
                    self.wallpapers.keys().cloned().collect()
                } else {
                    vec![output.clone()]
                };
                for output in outputs {
                    if !self.held.contains(&output)
                        && !self.held.contains(ALL_OUTPUTS)
                        && !current.iter().any(|(name, _)| name == &output || name == ALL_OUTPUTS)
                    {
                        db::playlist_assign_set(&transaction, &output, Some(*id))?;
                    }
                }
            }
            transaction.commit()
        })?;
        Ok(())
    }
}

pub(super) fn save_config(
    state: &Arc<WallState>,
    settings: &[(String, Value)],
) -> anyhow::Result<()> {
    state.reload_config();
    let mut root = state.config().root().clone();
    for (key, value) in settings {
        if !SAVED_KEYS.contains(&key.as_str()) {
            continue;
        }
        let (section, field) = key.split_once('.').expect("saved setting path");
        if value.is_null() {
            if let Some(section) = root.get_mut(section).and_then(Value::as_object_mut) {
                section.remove(field);
            }
        } else {
            if !root[section].is_object() {
                root[section] = Value::Object(serde_json::Map::new());
            }
            root[section][field] =
                skwd_config::schema::normalize_value(key, value).unwrap_or_else(|| value.clone());
        }
    }
    skwd_config::atomic_write(
        &skwd_wall_core::config::config_path(),
        &serde_json::to_vec_pretty(&root)?,
    )?;
    state.reload_config();
    Ok(())
}

pub(super) fn enabled_settings(state: &Arc<WallState>) -> Vec<(String, Value)> {
    let mut settings = vec![
        (weather::SOURCE.to_owned(), Value::from(weather::SOURCE_OUTDOORS)),
        (weather::ENABLED.to_owned(), Value::Bool(true)),
    ];
    if crate::infrastructure::semantic_index::available(state) {
        settings.push((semantic::ENABLED.to_owned(), Value::Bool(true)));
    }
    settings
}
