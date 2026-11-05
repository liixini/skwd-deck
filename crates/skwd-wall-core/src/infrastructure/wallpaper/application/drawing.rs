use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use paper_control::{
    ApplyRequest, DrawingColours, DrawingInk, DrawingTransition, StatusResult, TransitionPolicy,
};
use skwd_config::keys::drawing as keys;
use wall_proto::apply_fields::{ALL_OUTPUTS, PATH, TYPE};

use crate::infrastructure::paper::{
    PaperClient, PaperClientAdapter, PaperCompositionPlan, Startup,
};
use crate::state::WallState;

const FRAME_EXTENSION: &str = "ppm";
const PIN_DIRECTORY_PREFIX: &str = "drawing-";

struct DrawRequest<'a> {
    source: &'a Path,
    output: &'a str,
    targets: &'a [String],
    previous_static: Option<&'a std::collections::HashMap<String, String>>,
    artifact: PathBuf,
    media: Option<Media<'a>>,
}

struct Media<'a> {
    kind: &'a str,
    we_id: &'a str,
    mute: bool,
    volume: u32,
    reference: PathBuf,
}

struct MediaIdentity {
    key: String,
    source: PathBuf,
    live: PathBuf,
}

pub fn preview(state: &Arc<WallState>, key: &str, source: &Path, output: &str) -> Result<()> {
    let _guard = state.apply().lock();
    let config = state.config();
    let (artifact, media) = if key.starts_with(wall_proto::drawing::STATIC_KEY_PREFIX) {
        (crate::infrastructure::drawing::lookup(&config, source)?, None)
    } else {
        let signature = crate::infrastructure::drawing::media_signature(state, key, source)?;
        let (artifact, reference) =
            crate::infrastructure::drawing::lookup_media(&config, source, &signature)?;
        let we_id = key.strip_prefix(wall_proto::drawing::WE_KEY_PREFIX).unwrap_or("");
        let kind = if we_id.is_empty() { wall_proto::kind::VIDEO } else { wall_proto::kind::WE };
        (
            artifact,
            Some(Media {
                kind,
                we_id,
                mute: config.renderer().mute(),
                volume: config.renderer().volume(),
                reference,
            }),
        )
    };
    ensure!(
        play(
            state,
            &DrawRequest { source, output, targets: &[], previous_static: None, artifact, media }
        )?,
        "Drawing cannot preserve the current outputs"
    );
    Ok(())
}

pub(super) fn apply(
    state: &Arc<WallState>,
    source: &Path,
    output: &str,
    targets: &[String],
    previous_static: Option<&std::collections::HashMap<String, String>>,
) -> Result<bool> {
    let config = state.config().clone();
    if state.apply().no_transition()
        || locked_output(&config, output)
        || !enabled(state, config.root(), source)
    {
        return Ok(false);
    }
    let artifact = match crate::infrastructure::drawing::lookup(&config, source) {
        Ok(artifact) => artifact,
        Err(error) => {
            log::debug!("drawing: cache unavailable for {}: {error}", source.display());
            return Ok(false);
        }
    };
    play(state, &DrawRequest { source, output, targets, previous_static, artifact, media: None })
}

pub(super) fn apply_media(
    state: &WallState,
    request: crate::backend::wallpaper::ApplyOutputRequest<'_>,
) -> Result<bool> {
    let config = state.config();
    if state.apply().no_transition() || locked_output(&config, request.output) {
        return Ok(false);
    }
    let direct_key = match workshop_id(&config, request, Path::new(request.path)) {
        Some(id) => format!("{}{id}", wall_proto::drawing::WE_KEY_PREFIX),
        None => state.library_key(Path::new(request.path)).unwrap_or_default(),
    };
    if direct_key.is_empty() || !skwd_config::drawing_enabled(config.root(), &direct_key) {
        return Ok(false);
    }
    let identity = media_identity(state, request);
    if identity.key.is_empty() || !skwd_config::drawing_enabled(config.root(), &identity.key) {
        return Ok(false);
    }
    let signature = match crate::infrastructure::drawing::media_signature(
        state,
        &identity.key,
        &identity.source,
    ) {
        Ok(signature) => signature,
        Err(error) => {
            log::debug!(
                "drawing: media signature unavailable for {}: {error}",
                identity.source.display()
            );
            return Ok(false);
        }
    };
    let (artifact, reference) =
        match crate::infrastructure::drawing::lookup_media(&config, &identity.source, &signature) {
            Ok(found) => found,
            Err(error) => {
                log::debug!(
                    "drawing: media cache unavailable for {}: {error}",
                    identity.source.display()
                );
                return Ok(false);
            }
        };
    let media = Media {
        kind: request.kind,
        we_id: request.we_id,
        mute: request.mute,
        volume: request.volume,
        reference,
    };
    let targets = request.fanout.map_or(&[][..], |fanout| fanout.targets);
    let previous_static = request.fanout.and_then(|fanout| fanout.previous_static);
    play(
        state,
        &DrawRequest {
            source: &identity.live,
            output: request.output,
            targets,
            previous_static,
            artifact,
            media: Some(media),
        },
    )
}

fn locked_output(config: &crate::config::Config, output: &str) -> bool {
    output != ALL_OUTPUTS && config.display().output_locked(output)
}

fn workshop_id(
    config: &crate::config::Config,
    request: crate::backend::wallpaper::ApplyOutputRequest<'_>,
    path: &Path,
) -> Option<String> {
    if !request.we_id.is_empty() {
        return Some(request.we_id.to_string());
    }
    let root = config.we_dir();
    let root = root.canonicalize().unwrap_or(root);
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let id = path.strip_prefix(&root).ok()?.components().next()?.as_os_str().to_str()?;
    crate::we::valid_we_id(id).then(|| id.to_string())
}

fn media_identity(
    state: &WallState,
    request: crate::backend::wallpaper::ApplyOutputRequest<'_>,
) -> MediaIdentity {
    let config = state.config();
    let original = PathBuf::from(request.path);
    if let Some(we_id) = workshop_id(&config, request, &original) {
        let source = config.we_dir().join(&we_id);
        let source = source.canonicalize().unwrap_or(source);
        let live = if request.kind == wall_proto::kind::VIDEO {
            PathBuf::from(request.path)
        } else {
            source.clone()
        };
        return MediaIdentity {
            key: format!("{}{we_id}", wall_proto::drawing::WE_KEY_PREFIX),
            source,
            live,
        };
    }
    let source = original.canonicalize().unwrap_or(original);
    let key = state.library_key(&source).unwrap_or_default();
    MediaIdentity { key, live: source.clone(), source }
}

fn enabled(state: &WallState, config: &serde_json::Value, source: &Path) -> bool {
    let key = state.library_key(source).unwrap_or_default();
    skwd_config::drawing_enabled(config, &key)
}

fn play(state: &WallState, request: &DrawRequest<'_>) -> Result<bool> {
    crate::plasma::require_backend()?;
    if crate::plasma::available() {
        return play_plasma(state, request);
    }
    let policy_signature = crate::apply::paper_policy_signature(state);
    let config = state.config().clone();
    let outputs = crate::outputs::enumerate();
    let previous = crate::audio::read_state(&config.cache_dir());
    let actual = state.renderers().assignments();
    let (mut apply, candidate, selected) = plan(state, &outputs, &previous, &actual, request)?;
    let adapter = PaperClientAdapter::configured(&config);
    let client = PaperClient::configured(&config);
    let status = client.status(Startup::Allowed)?;
    if status.assignments.is_empty() && crate::infrastructure::paper::stationary_wallpaper(&config)
    {
        client.set_surface_overrides(crate::infrastructure::paper::stationary_surfaces(&config))?;
    }
    if !scope(&mut apply, &selected, outputs.len(), &status) {
        return Ok(false);
    }
    if !capture_sources_ready(
        state,
        &actual,
        &selected,
        &apply,
        &status,
        &crate::we::handoff_directory(state),
    ) {
        log::warn!("drawing: current media frame unavailable, using ordinary apply");
        return Ok(false);
    }
    let _pinned = pin_transition_sources(state, &mut apply)?;
    let replace_all = apply.replace_all;
    let updates_policy = apply.policy.is_some();
    state.renderers().sync_paper_controls()?;
    adapter.apply_request(apply)?;
    if !replace_all && let Err(error) = state.renderers().retire_shared_outputs(&selected) {
        log::warn!(
            "drawing: committed wallpaper could not retire previous views {selected:?}: {error:#}"
        );
    }
    crate::audio::write_state(&config.cache_dir(), &candidate);
    if updates_policy {
        state.renderers().set_policy(crate::apply::PAPER_POLICY_KEY, &policy_signature);
    }
    if replace_all {
        crate::awww::stop();
        state.renderers().kill_all();
        let assignments = outputs
            .into_iter()
            .filter_map(|output| {
                let entry = candidate.get(&output.name)?;
                let path = if entry.get(TYPE)?.as_str()? == wall_proto::kind::WE {
                    config
                        .we_dir()
                        .join(entry.get(wall_proto::apply_fields::WE_ID)?.as_str()?)
                        .display()
                        .to_string()
                } else {
                    entry.get(PATH)?.as_str()?.to_string()
                };
                Some((output.name, path))
            })
            .collect();
        state.renderers().replace_assignments(assignments);
        return Ok(true);
    }
    for output in &selected {
        state.renderers().kill_video_paper(output);
        state.renderers().kill_output_still(output);
    }
    record_assignments(state, request, &candidate, &selected);
    Ok(true)
}

fn play_plasma(state: &WallState, request: &DrawRequest<'_>) -> Result<bool> {
    let config = state.config().clone();
    let outputs = crate::outputs::enumerate();
    let previous = crate::audio::read_state(&config.cache_dir());
    let actual = state.renderers().assignments();
    let plan = plasma_plan(state, &outputs, &previous, &actual, request)?;
    let map = plan.candidate.as_object().context("audio state must be an object")?;
    crate::plasma::apply(state, &outputs, map, &plan.transitions)?;
    if let Err(error) = crate::plasma::apply(state, &outputs, map, &BTreeMap::new()) {
        log::warn!("drawing: Plasma kept the played drawing in its configuration: {error:#}");
    }
    crate::audio::write_state(&config.cache_dir(), &plan.candidate);
    record_assignments(state, request, &plan.candidate, &plan.selected);
    crate::plasma::retire_native(state);
    crate::apply::record_paper_policy(state);
    Ok(true)
}

fn record_assignments(
    state: &WallState,
    request: &DrawRequest<'_>,
    candidate: &serde_json::Value,
    selected: &[String],
) {
    for output in selected {
        let Some(path) = candidate
            .get(output)
            .and_then(|entry| entry.get(PATH))
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        let actual =
            if request.media.as_ref().is_some_and(|media| media.kind == wall_proto::kind::WE) {
                request.source.to_str().unwrap_or(path)
            } else {
                path
            };
        state.renderers().set_assignment(output, actual);
    }
}

fn capture_sources_ready(
    state: &WallState,
    actual: &std::collections::HashMap<String, String>,
    selected: &[String],
    apply: &ApplyRequest,
    status: &StatusResult,
    handoff_directory: &Path,
) -> bool {
    selected.iter().all(|output| {
        if !snapshot_required(state, actual, output)
            || (state.renderers().renderer_capture_key(output).is_none()
                && status.assignments.iter().any(|assignment| {
                    assignment.outputs.iter().any(|name| name == output || name == ALL_OUTPUTS)
                }))
        {
            return true;
        }
        apply.assignments.iter().any(|assignment| {
            assignment.outputs.iter().any(|name| name == output)
                && assignment
                    .transition
                    .as_ref()
                    .and_then(|transition| transition.from.as_deref())
                    .is_some_and(|from| {
                        let path = Path::new(from);
                        path.parent() == Some(handoff_directory) && path.is_file()
                    })
        })
    })
}

fn snapshot_required(
    state: &WallState,
    actual: &std::collections::HashMap<String, String>,
    output: &str,
) -> bool {
    if state.renderers().renderer_capture_key(output).is_some() {
        return true;
    }
    actual.get(output).or_else(|| actual.get(ALL_OUTPUTS)).is_some_and(|source| {
        let surface = crate::infrastructure::paper::stationary_surface(
            &state.config(),
            paper_control::SourceKind::Static,
        );
        paper_control::is_video_path(source)
            || Path::new(source).is_dir()
            || surface.blur > 0
            || surface.dim > 0
            || (crate::we::valid_we_id(source)
                && state.renderers().scene_paper_key_for(output).is_some())
    })
}

fn pin_transition_sources(
    state: &WallState,
    apply: &mut ApplyRequest,
) -> Result<Option<tempfile::TempDir>> {
    let directory = crate::we::handoff_directory(state);
    let mut pinned = None;
    for (index, assignment) in apply.assignments.iter_mut().enumerate() {
        let Some(transition) = assignment.transition.as_mut() else { continue };
        let Some(source) = transition.from.as_ref().map(PathBuf::from) else { continue };
        if source.parent() != Some(directory.as_path()) {
            continue;
        }
        if pinned.is_none() {
            pinned =
                Some(tempfile::Builder::new().prefix(PIN_DIRECTORY_PREFIX).tempdir_in(&directory)?);
        }
        let target = pinned.as_ref().unwrap().path().join(format!("{index}.{FRAME_EXTENSION}"));
        if std::fs::hard_link(&source, &target).is_err() {
            std::fs::copy(&source, &target)?;
        }
        transition.from = Some(target.to_string_lossy().into_owned());
    }
    Ok(pinned)
}

fn scope(
    apply: &mut ApplyRequest,
    selected: &[String],
    output_count: usize,
    status: &StatusResult,
) -> bool {
    if selected.len() == output_count {
        return true;
    }
    if !status.assignments.is_empty()
        && status
            .assignments
            .iter()
            .any(|assignment| assignment.outputs.iter().any(|output| output == ALL_OUTPUTS))
    {
        return false;
    }
    apply
        .assignments
        .retain(|assignment| assignment.outputs.iter().any(|output| selected.contains(output)));
    apply.replace_all = false;
    if !status.assignments.is_empty() {
        apply.policy = None;
    }
    true
}

struct Style {
    colours: DrawingColours,
    ink: DrawingInk,
    duration_ms: u64,
}

struct PlasmaPlan {
    candidate: serde_json::Value,
    transitions: BTreeMap<String, TransitionPolicy>,
    selected: Vec<String>,
}

fn plan(
    state: &WallState,
    outputs: &[crate::outputs::OutputInfo],
    previous: &serde_json::Value,
    actual: &HashMap<String, String>,
    request: &DrawRequest<'_>,
) -> Result<(paper_control::ApplyRequest, serde_json::Value, Vec<String>)> {
    let config = state.config().clone();
    let (candidate, selected) = choose(&config, outputs, previous, request)?;
    let adapter = PaperClientAdapter::configured(&config);
    let PaperCompositionPlan::Replace(mut apply) =
        adapter.composition_plan(&config, &candidate, outputs)?
    else {
        anyhow::bail!("drawing composition has no assignments");
    };
    let style = style(&config)?;
    let fps = (config.transition().fps() > 0).then(|| config.transition().fps());
    for assignment in &mut apply.assignments {
        let Some(output) = assignment.outputs.first().filter(|output| selected.contains(output))
        else {
            continue;
        };
        let live_from = snapshot_required(state, actual, output)
            .then(|| {
                crate::we::capture_transition_frame(state, output, crate::we::CaptureFrame::Surface)
            })
            .flatten();
        let from = live_from
            .as_deref()
            .or_else(|| {
                actual
                    .get(output)
                    .or_else(|| actual.get(ALL_OUTPUTS))
                    .map(String::as_str)
                    .filter(|path| Path::new(path).is_file() && !paper_control::is_video_path(path))
            })
            .or_else(|| earlier_still(request, previous, output));
        let mut transition = policy(request, &style, from)?;
        transition.fps = fps;
        assignment.transition = Some(transition);
    }
    if let Some(policy) = &mut apply.policy {
        policy.transition_fps = policy.transition_fps.filter(|fps| *fps > 0);
    }
    Ok((apply, candidate, selected))
}

fn plasma_plan(
    state: &WallState,
    outputs: &[crate::outputs::OutputInfo],
    previous: &serde_json::Value,
    actual: &HashMap<String, String>,
    request: &DrawRequest<'_>,
) -> Result<PlasmaPlan> {
    let config = state.config().clone();
    let (candidate, selected) = choose(&config, outputs, previous, request)?;
    let style = style(&config)?;
    let mut transitions = BTreeMap::new();
    for output in &selected {
        let live_from = actual
            .get(output)
            .or_else(|| actual.get(ALL_OUTPUTS))
            .and_then(|shown| crate::apply::previous_media_source(state, output, shown));
        let from = live_from.as_deref().or_else(|| earlier_still(request, previous, output));
        transitions.insert(output.clone(), policy(request, &style, from)?);
    }
    Ok(PlasmaPlan { candidate, transitions, selected })
}

fn choose(
    config: &crate::config::Config,
    outputs: &[crate::outputs::OutputInfo],
    previous: &serde_json::Value,
    request: &DrawRequest<'_>,
) -> Result<(serde_json::Value, Vec<String>)> {
    let selected = outputs
        .iter()
        .filter(|output| {
            (if request.targets.is_empty() {
                request.output == ALL_OUTPUTS || output.name == request.output
            } else {
                request.targets.iter().any(|target| target == ALL_OUTPUTS || target == &output.name)
            }) && !config.display().output_locked(&output.name)
        })
        .map(|output| output.name.clone())
        .collect::<Vec<_>>();
    ensure!(!selected.is_empty(), "Drawing requires an unlocked live output");
    let source = request.source.to_str().context("drawing source is not UTF-8")?;
    let mut candidate = previous.clone();
    let entries = candidate.as_object_mut().context("audio state must be an object")?;
    if let Some(wildcard) = entries.remove(ALL_OUTPUTS) {
        for output in outputs {
            entries.entry(output.name.clone()).or_insert_with(|| wildcard.clone());
        }
    }
    for output in &selected {
        let kind = request.media.as_ref().map_or(wall_proto::kind::STATIC, |media| media.kind);
        let we_id = request.media.as_ref().map_or("", |media| media.we_id);
        let mute = request.media.as_ref().is_none_or(|media| media.mute);
        let volume =
            request.media.as_ref().map_or(config.renderer().volume(), |media| media.volume);
        entries.insert(
            output.clone(),
            crate::audio::entry(
                kind,
                if kind == wall_proto::kind::WE { "" } else { source },
                we_id,
                mute,
                volume,
            ),
        );
    }
    Ok((candidate, selected))
}

fn earlier_still<'a>(
    request: &'a DrawRequest<'_>,
    previous: &'a serde_json::Value,
    output: &str,
) -> Option<&'a str> {
    request
        .previous_static
        .and_then(|sources| sources.get(output).or_else(|| sources.get(ALL_OUTPUTS)))
        .map(String::as_str)
        .filter(|path| Path::new(path).is_file())
        .or_else(|| {
            previous
                .get(output)
                .or_else(|| previous.get(ALL_OUTPUTS))
                .filter(|entry| {
                    entry.get(TYPE).and_then(serde_json::Value::as_str)
                        == Some(wall_proto::kind::STATIC)
                })
                .and_then(|entry| entry.get(PATH))
                .and_then(serde_json::Value::as_str)
                .filter(|path| Path::new(path).is_file())
        })
        .or_else(|| request.media.as_ref().and_then(|media| media.reference.to_str()))
}

fn style(config: &crate::config::Config) -> Result<Style> {
    let duration_ms = skwd_config::schema::read_number(config.root(), keys::DURATION_MS)
        .context("drawing duration setting missing")? as u64;
    let fallback = DrawingColours::default();
    let colours = DrawingColours {
        paper: colour(config.root(), keys::PAPER_COLOR, fallback.paper),
        ink: colour(config.root(), keys::INK_COLOR, fallback.ink),
    };
    let ink = match skwd_config::schema::read_text(config.root(), keys::INK).as_deref() {
        Some(keys::INK_RAINBOW) => DrawingInk::Rainbow,
        Some(keys::INK_PLAIN) | None => DrawingInk::Plain,
        Some(other) => {
            log::warn!("drawing: {} {other:?} is not an ink", keys::INK);
            DrawingInk::Plain
        }
    };
    Ok(Style { colours, ink, duration_ms })
}

fn policy(
    request: &DrawRequest<'_>,
    style: &Style,
    from: Option<&str>,
) -> Result<TransitionPolicy> {
    let source = request.source.to_str().context("drawing source is not UTF-8")?;
    Ok(TransitionPolicy {
        drawing: Some(DrawingTransition {
            artifact: request.artifact.clone(),
            colours: style.colours,
            ink: style.ink,
        }),
        drawing_source: request.media.as_ref().map(|media| media.reference.clone()),
        from: Some(from.unwrap_or(source).to_string()),
        effect: Some(paper_control::DRAWING_EFFECT.to_string()),
        duration_ms: Some(style.duration_ms),
        fps: None,
    })
}

fn colour(root: &serde_json::Value, key: &str, fallback: [u8; 3]) -> [u8; 3] {
    let text = skwd_config::schema::read_text(root, key);
    text.as_deref().and_then(skwd_config::parse_rgb).unwrap_or_else(|| {
        log::warn!("drawing: {key} {text:?} is not a colour");
        fallback
    })
}

#[cfg(test)]
#[path = "drawing/tests.rs"]
mod tests;
