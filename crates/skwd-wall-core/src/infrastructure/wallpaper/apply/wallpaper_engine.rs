use crate::state::WallState;

use super::resolver::resolve_we_from_state;

pub fn reload_we(state: &WallState) -> anyhow::Result<()> {
    crate::plasma::require_backend()?;
    let cache = state.config().cache_dir();
    let current = crate::audio::read_state(&cache);
    let Some(map) = current.as_object() else {
        return Ok(());
    };
    let (groups, audio) = resolve_we_from_state(map);
    if groups.is_empty() {
        return Ok(());
    }
    if crate::plasma::available() {
        crate::plasma::apply_current(state)?;
        crate::plasma::retire_native(state);
        return Ok(());
    }
    rebuild_we(state, groups, audio)
}

pub(super) enum PreparedWeMode<'a> {
    Keep { audio_changed: bool },
    Replace(Vec<crate::we::NativeSceneCandidate<'a>>),
}

enum CommitReadyWeMode<'a> {
    Keep { audio_changed: bool },
    Replace(Vec<crate::we::PreparedNativeSceneCandidate<'a>>),
}

pub(super) struct PreparedWe<'a> {
    pub(super) groups: std::collections::BTreeMap<String, Vec<String>>,
    pub(super) audio: std::collections::BTreeMap<String, (bool, u32)>,
    pub(super) mode: PreparedWeMode<'a>,
}

pub(super) struct CommitReadyWe<'a> {
    groups: std::collections::BTreeMap<String, Vec<String>>,
    audio: std::collections::BTreeMap<String, (bool, u32)>,
    mode: CommitReadyWeMode<'a>,
}

impl<'a> PreparedWe<'a> {
    pub(super) fn prepare_commit(self) -> anyhow::Result<CommitReadyWe<'a>> {
        let mode = match self.mode {
            PreparedWeMode::Keep { audio_changed } => CommitReadyWeMode::Keep { audio_changed },
            PreparedWeMode::Replace(native) => {
                CommitReadyWeMode::Replace(crate::we::prepare_cold_scene_set(native)?)
            }
        };
        Ok(CommitReadyWe { groups: self.groups, audio: self.audio, mode })
    }
}

impl CommitReadyWe<'_> {
    pub(super) fn finalize(self, state: &WallState) {
        let group_count = self.groups.len();
        match self.mode {
            CommitReadyWeMode::Keep { audio_changed } => {
                if audio_changed {
                    for (we_id, outputs) in &self.groups {
                        let (mute, volume) = self.audio.get(we_id).copied().unwrap_or((true, 100));
                        state.renderers().send_audio(Some(outputs), Some(mute), Some(volume));
                    }
                    state.renderers().set_we_render(self.groups, self.audio);
                }
                log::info!(
                    "reconcile: WE scene set unchanged ({} group(s)), {}",
                    group_count,
                    if audio_changed { "audio updated in place" } else { "not reloading" }
                );
            }
            CommitReadyWeMode::Replace(native) => {
                crate::we::finalize_scene_set(state, native);
                state.renderers().set_we_render(self.groups, self.audio);
            }
        }
    }
}

pub(super) fn group_output(
    output: &str,
    we_id: &str,
    groups: &mut std::collections::BTreeMap<String, Vec<String>>,
) {
    log::info!("reconcile {output}: we {we_id}");
    groups.entry(we_id.to_string()).or_default().push(output.to_string());
}

pub(super) fn prepare_we(
    state: &WallState,
    mut groups: std::collections::BTreeMap<String, Vec<String>>,
    audio: std::collections::BTreeMap<String, (bool, u32)>,
) -> anyhow::Result<PreparedWe<'_>> {
    for outputs in groups.values_mut() {
        outputs.sort();
    }
    let native_alive = groups
        .values()
        .filter(|outputs| {
            let key = crate::we::scene_renderer_key(outputs);
            state.renderers().is_scene_paper(&key) && state.renderers().has_video_paper(&key)
        })
        .count();
    let coverage = native_alive >= groups.len();
    let policy_matches =
        coverage && (native_alive == 0 || super::policy::native_scene_policy_matches(state));
    if policy_matches && state.renderers().we_render_groups_match(&groups) {
        let audio_changed = !state.renderers().we_render_matches(&groups, &audio);
        return Ok(PreparedWe { groups, audio, mode: PreparedWeMode::Keep { audio_changed } });
    }
    let mut native = Vec::new();
    for (we_id, outputs) in &groups {
        let (mute, volume) = audio.get(we_id).copied().unwrap_or((true, 100));
        // Warm swap mutates a live renderer and cannot be rolled back as a batch.
        native.push(crate::we::spawn_scene_for(state, outputs, we_id, mute, volume, false)?);
    }
    Ok(PreparedWe { groups, audio, mode: PreparedWeMode::Replace(native) })
}

pub(super) fn rebuild_we(
    state: &WallState,
    groups: std::collections::BTreeMap<String, Vec<String>>,
    audio: std::collections::BTreeMap<String, (bool, u32)>,
) -> anyhow::Result<()> {
    let prepared = prepare_we(state, groups, audio)?.prepare_commit()?;
    prepared.finalize(state);
    Ok(())
}
