use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::config::Config;
use crate::outputs::OutputInfo;
use crate::state::WallState;

use paper_control::{
    ApplyRequest, ApplyResult, Assignment, FillMode, Layer, RendererPolicy, SandPolicy,
    SandQuality, SandScope, ScenePolicy, Source, StopResult,
};

use super::adapter::{PaperClientAdapter, assignment_with_options, fill_mode, video_engine};

const PERF_SCENE_FPS: u32 = 30;
const PERF_SCENE_MAX_DIMENSION: u32 = 2048;
const PERF_SCENE_EFFECT_CHAINS: u32 = 4;
const PERF_SCENE_EFFECT_PASSES: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaperCompositionPlan {
    Replace(ApplyRequest),
    StopAll,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaperCompositionResult {
    Applied(ApplyResult),
    Stopped(StopResult),
}

impl PaperClientAdapter {
    pub fn composition_plan(
        &self,
        config: &Config,
        state: &Value,
        live_outputs: &[OutputInfo],
    ) -> Result<PaperCompositionPlan> {
        Self::composition_plan_with(config, state, live_outputs, |path| {
            Ok(Source::video(path, Some(video_engine(&config.renderer().video_engine()))))
        })
    }

    pub fn tinier_composition_plan(
        &self,
        wall: &WallState,
        state: &Value,
        live_outputs: &[OutputInfo],
    ) -> Result<PaperCompositionPlan> {
        let config = wall.config();
        Self::composition_plan_with(&config, state, live_outputs, |path| {
            tinier_or_default_source(wall, path)
        })
    }

    fn composition_plan_with(
        config: &Config,
        state: &Value,
        live_outputs: &[OutputInfo],
        video_source: impl Fn(&str) -> Result<Source>,
    ) -> Result<PaperCompositionPlan> {
        let entries = selected_entries(config, state, live_outputs)?;
        if entries.is_empty() {
            return Ok(PaperCompositionPlan::StopAll);
        }
        let dedup_state = Value::Object(entries.clone().into_iter().collect());
        let dedup_mute: BTreeSet<String> =
            crate::audio::compute_dedup(&dedup_state).into_iter().collect();
        let mut assignments = Vec::with_capacity(entries.len());
        for (output, entry) in entries {
            assignments.push(composition_assignment(
                config,
                &output,
                &entry,
                &dedup_mute,
                live_outputs,
                &video_source,
            )?);
        }
        let request = ApplyRequest {
            assignments,
            replace_all: true,
            policy: Some(renderer_policy(config, live_outputs)),
        };
        request.validate().map_err(|error| anyhow::anyhow!("Paper {error}"))?;
        Ok(PaperCompositionPlan::Replace(request))
    }

    pub fn reconcile_composition(
        &self,
        config: &Config,
        state: &Value,
        live_outputs: &[OutputInfo],
    ) -> Result<PaperCompositionResult> {
        match self.composition_plan(config, state, live_outputs)? {
            PaperCompositionPlan::Replace(request) => {
                self.client.apply(request).map(PaperCompositionResult::Applied)
            }
            PaperCompositionPlan::StopAll => {
                self.client.stop(Vec::new()).map(PaperCompositionResult::Stopped)
            }
        }
    }

    pub fn reconcile_current_composition(
        &self,
        config: &Config,
        live_outputs: &[OutputInfo],
    ) -> Result<PaperCompositionResult> {
        let state = crate::audio::read_state(&config.cache_dir());
        self.reconcile_composition(config, &state, live_outputs)
    }
}

pub fn renderer_policy(config: &Config, outputs: &[OutputInfo]) -> RendererPolicy {
    let performance_mode = config.renderer().performance_mode();
    let configured_fps = config.renderer().we_fps();
    let quality = match config.transition().sand_quality().as_str() {
        "full" => SandQuality::Full,
        "low" => SandQuality::Low,
        _ => SandQuality::Auto,
    };
    let shader = config.transition().shader();
    let scope = if shader.starts_with("sand-") && config.transition().scope(&shader) == "primary" {
        SandScope::Primary
    } else {
        SandScope::All
    };
    let output_fps = outputs
        .iter()
        .map(|output| (output.name.clone(), output_policy_fps(configured_fps, output.refresh_mhz)))
        .collect();
    RendererPolicy {
        idle_seconds: Some(config.renderer().idle_pause_seconds()),
        transitions_enabled: Some(config.transition().active()),
        sand: Some(SandPolicy {
            quality: Some(quality),
            scope: Some(scope),
            primary: non_empty(config.transition().sand_primary()),
            sharp: Some(config.transition().sand_sharp()),
            fps: config.transition().sand_fps().parse().ok(),
        }),
        scene: Some(ScenePolicy {
            fps: Some(scene_policy_fps(configured_fps, performance_mode)),
            disable_particles: Some(config.renderer().we_disable_particles()),
            assets_dir: non_empty(config.we_assets_dir()),
            max_dimension: performance_mode.then_some(PERF_SCENE_MAX_DIMENSION),
            max_effect_chains: performance_mode.then_some(PERF_SCENE_EFFECT_CHAINS),
            max_effect_passes: performance_mode.then_some(PERF_SCENE_EFFECT_PASSES),
            strict: Some(false),
        }),
        output_fps,
    }
}

pub(super) fn scene_policy_fps(configured_fps: u32, performance_mode: bool) -> u32 {
    let fps = configured_fps.clamp(1, 240);
    if performance_mode { fps.min(PERF_SCENE_FPS) } else { fps }
}

pub(super) fn output_policy_fps(configured_fps: u32, refresh_mhz: i32) -> u32 {
    crate::outputs::effective_fps(configured_fps, refresh_mhz).min(1000)
}

fn composition_assignment(
    config: &Config,
    output: &str,
    entry: &Value,
    dedup_mute: &BTreeSet<String>,
    live_outputs: &[OutputInfo],
    video_source: &impl Fn(&str) -> Result<Source>,
) -> Result<Assignment> {
    let entry = entry
        .as_object()
        .with_context(|| format!("Paper composition entry for {output} must be an object"))?;
    let kind = entry
        .get("type")
        .and_then(Value::as_str)
        .with_context(|| format!("Paper composition entry for {output} is missing type"))?;
    let source = match kind {
        wall_proto::kind::STATIC => Source::static_file(entry_path(entry, output)?),
        wall_proto::kind::VIDEO => video_source(entry_path(entry, output)?)?,
        wall_proto::kind::WE => {
            let we_id = entry
                .get("we_id")
                .and_then(Value::as_str)
                .filter(|we_id| crate::we::valid_we_id(we_id))
                .with_context(|| {
                    format!("Paper composition entry for {output} has an invalid WE id")
                })?;
            Source::wallpaper_engine(config.we_dir().join(we_id).to_string_lossy().into_owned())
        }
        kind => bail!("unsupported Paper composition source kind {kind} for {output}"),
    };
    let mute = source.effective_video_engine() == Some(paper_control::VideoEngine::Tinier)
        || dedup_mute.contains(output)
        || entry.get("mute").and_then(Value::as_bool).unwrap_or_else(|| config.renderer().mute());
    let volume = entry
        .get("volume")
        .and_then(Value::as_u64)
        .map_or_else(|| config.renderer().volume(), |volume| volume.min(100) as u32);
    let fill_mode = if output == "*" {
        live_outputs.first().map_or_else(FillMode::default, |output| {
            fill_mode(&config.display().fill_mode_for(&output.name))
        })
    } else {
        fill_mode(&config.display().fill_mode_for(output))
    };
    Ok(assignment_with_options(
        vec![output.to_string()],
        source,
        fill_mode,
        mute,
        volume,
        Layer::Background,
    ))
}

pub(crate) fn tinier_or_default_source(wall: &WallState, path: &str) -> Result<Source> {
    let (original, entry) = wall
        .with_db(|connection| {
            let source = crate::db::tinier_convert_src(connection, path)?;
            let original = source.unwrap_or_else(|| path.to_string());
            let entry = crate::db::tinier_convert_entry(connection, &original)?;
            Ok((original, entry))
        })
        .context("read Tinier conversion record")?;
    let Some((destination, frame_rate, preset, original_size)) = entry else {
        return Ok(Source::video(original, Some(paper_control::VideoEngine::Default)));
    };
    let source_size = std::fs::metadata(&original).map_or(0, |metadata| metadata.len() as i64);
    let valid = std::fs::metadata(&destination).is_ok_and(|metadata| {
        metadata.is_file()
            && metadata.len() > 0
            && metadata.len() <= crate::db::TINIER_CONVERT_MAX_BYTES
    });
    if preset != crate::db::TINIER_CONVERT_PRESET || original_size != source_size || !valid {
        return Ok(Source::video(original, Some(paper_control::VideoEngine::Default)));
    }
    Ok(Source::tinier_video(destination, frame_rate))
}

fn selected_entries(
    config: &Config,
    state: &Value,
    live_outputs: &[OutputInfo],
) -> Result<BTreeMap<String, Value>> {
    let map = state.as_object().context("Paper composition state must be an object")?;
    if live_outputs.is_empty() {
        return Ok(BTreeMap::new());
    }
    let outputs: BTreeSet<&str> = live_outputs.iter().map(|output| output.name.as_str()).collect();
    let wildcard = map.get("*");
    let has_live_override = map.keys().any(|output| outputs.contains(output.as_str()));
    if let Some(entry) = wildcard
        && !has_live_override
        && has_uniform_fill(config, live_outputs)
    {
        return Ok(BTreeMap::from([("*".to_string(), entry.clone())]));
    }
    let selected = outputs
        .into_iter()
        .filter_map(|output| {
            map.get(output).or(wildcard).map(|entry| (output.to_string(), entry.clone()))
        })
        .collect();
    Ok(selected)
}

fn has_uniform_fill(config: &Config, outputs: &[OutputInfo]) -> bool {
    let mut fills =
        outputs.iter().map(|output| fill_mode(&config.display().fill_mode_for(&output.name)));
    let Some(first) = fills.next() else {
        return true;
    };
    fills.all(|fill| fill == first)
}

fn entry_path<'a>(entry: &'a serde_json::Map<String, Value>, output: &str) -> Result<&'a str> {
    entry
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .with_context(|| format!("Paper composition entry for {output} is missing path"))
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}
