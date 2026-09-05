use crate::state::WallState;
use crate::{apply, outputs};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::infrastructure::renderers::kill_held_renderer;

const SCENE_FREEZE_TIMEOUT: Duration = Duration::from_secs(5);
static SCENE_FREEZE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) struct NativeSceneCandidate<'a> {
    key: String,
    renderer: Option<apply::ReadyRenderer<'a>>,
    properties: (String, serde_json::Map<String, serde_json::Value>),
}

pub(crate) struct PreparedNativeSceneCandidate<'a> {
    key: String,
    renderer: Option<apply::PreparedRenderer<'a>>,
    properties: (String, serde_json::Map<String, serde_json::Value>),
}

impl<'a> NativeSceneCandidate<'a> {
    fn prepare_commit(self) -> anyhow::Result<PreparedNativeSceneCandidate<'a>> {
        Ok(PreparedNativeSceneCandidate {
            key: self.key,
            renderer: self.renderer.map(apply::ReadyRenderer::prepare_commit).transpose()?,
            properties: self.properties,
        })
    }
}

pub(crate) fn prepare_scene_set(
    native: Vec<NativeSceneCandidate<'_>>,
) -> anyhow::Result<Vec<PreparedNativeSceneCandidate<'_>>> {
    native.into_iter().map(NativeSceneCandidate::prepare_commit).collect()
}

pub(crate) fn prepare_cold_scene_set(
    native: Vec<NativeSceneCandidate<'_>>,
) -> anyhow::Result<Vec<PreparedNativeSceneCandidate<'_>>> {
    if native.iter().any(|candidate| candidate.renderer.is_none()) {
        anyhow::bail!("warm-applied native scene cannot join a transactional renderer batch");
    }
    prepare_scene_set(native)
}

pub(crate) fn finalize_scene_set(state: &WallState, native: Vec<PreparedNativeSceneCandidate<'_>>) {
    let has_native = !native.is_empty();
    let native_keys = native.iter().map(|candidate| candidate.key.clone()).collect::<Vec<_>>();
    let signature = apply::scene_properties_signature(
        &native.iter().map(|candidate| candidate.properties.clone()).collect::<Vec<_>>(),
    );
    for candidate in native {
        if let Some(renderer) = candidate.renderer {
            renderer.finalize();
        }
    }
    state.renderers().replace_holders(Vec::new());
    state.renderers().retain_scene_papers(&native_keys);
    if has_native {
        apply::record_native_scene_policies(state);
        apply::record_scene_properties(state, &signature);
    }
}

pub(crate) fn commit_scene_set(
    state: &WallState,
    native: Vec<NativeSceneCandidate<'_>>,
) -> anyhow::Result<()> {
    let native = prepare_scene_set(native)?;
    finalize_scene_set(state, native);
    Ok(())
}

pub fn read_project_type(item_dir: &Path) -> (String, String) {
    let path = item_dir.join("project.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return ("scene".to_string(), String::new());
    };
    let val: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    let ty = val.get("type").and_then(|node| node.as_str()).unwrap_or("scene").to_lowercase();
    let file = val.get("file").and_then(|node| node.as_str()).unwrap_or("").to_string();
    (ty, file)
}

pub fn is_video_project(proj_type: &str, file: &str) -> bool {
    proj_type.eq_ignore_ascii_case("video") && !file.is_empty()
}

pub fn is_supported_project(proj_type: &str) -> bool {
    proj_type.eq_ignore_ascii_case("scene") || proj_type.eq_ignore_ascii_case("video")
}

pub fn read_project_title(item_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(item_dir.join("project.json")).ok()?;
    let val: serde_json::Value = serde_json::from_str(&text).ok()?;
    val.get("title")
        .and_then(|node| node.as_str())
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
}

pub fn find_preview(item_dir: &Path) -> Option<PathBuf> {
    let project = std::fs::read(item_dir.join("project.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    if let Some(preview) = project
        .as_ref()
        .and_then(|project| project.get("preview"))
        .and_then(serde_json::Value::as_str)
        .and_then(|file| safe_item_join(item_dir, file))
        .filter(|path| path.is_file())
    {
        return Some(preview);
    }
    let mut previews: Vec<_> = std::fs::read_dir(item_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().to_lowercase().starts_with("preview."))
        .filter_map(|entry| safe_item_join(item_dir, &entry.file_name().to_string_lossy()))
        .filter(|path| path.is_file())
        .collect();
    previews.sort();
    previews.into_iter().next()
}

pub(crate) fn spawn_scene_for<'a>(
    state: &'a WallState,
    outputs: &[String],
    we_id: &str,
    mute: bool,
    volume: u32,
    allow_warm_swap: bool,
) -> anyhow::Result<NativeSceneCandidate<'a>> {
    if !valid_we_id(we_id) {
        anyhow::bail!("invalid WE id: {we_id}");
    }
    let item_dir = state.config().we_dir().join(we_id);
    let properties = scene_overrides(state, we_id);
    native_scene(state, outputs, &item_dir, we_id, &properties, mute, volume, allow_warm_swap)
}

pub(crate) fn scene_overrides(
    state: &WallState,
    we_id: &str,
) -> serde_json::Map<String, serde_json::Value> {
    state
        .database()
        .with_connection(|connection| Ok(crate::db::we_properties(connection, we_id)))
        .unwrap_or_default()
}

pub(crate) fn scene_renderer_key(outputs: &[String]) -> String {
    let mut selected: Vec<String> = outputs
        .iter()
        .filter(|output| !output.is_empty() && output.as_str() != "*")
        .cloned()
        .collect();
    selected.sort();
    selected.dedup();
    if selected.is_empty() { "*".to_string() } else { selected.join(",") }
}

pub fn valid_we_id(we_id: &str) -> bool {
    !we_id.is_empty()
        && !we_id.starts_with('-')
        && !we_id.contains('/')
        && !we_id.contains('\\')
        && !we_id.contains("..")
        && we_id != "."
}

pub fn safe_item_join(item_dir: &std::path::Path, file: &str) -> Option<std::path::PathBuf> {
    let rel = std::path::Path::new(file);
    if rel.components().any(|comp| {
        matches!(
            comp,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return None;
    }
    let root = std::fs::canonicalize(item_dir).ok()?;
    let joined = std::fs::canonicalize(item_dir.join(rel)).ok()?;
    joined.starts_with(&root).then_some(joined)
}

pub(crate) fn capture_transition_frame(state: &WallState, output: &str) -> Option<String> {
    capture_transition_frame_with_timeout(state, output, SCENE_FREEZE_TIMEOUT)
}

pub(crate) fn capture_transition_frame_with_timeout(
    state: &WallState,
    output: &str,
    timeout: Duration,
) -> Option<String> {
    let directory = PathBuf::from(state.config().cache_dir()).join("scene-handoffs");
    if let Err(error) = std::fs::create_dir_all(&directory) {
        log::warn!("scene handoff: cannot create {} ({error})", directory.display());
        return None;
    }
    let sequence = SCENE_FREEZE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = directory.join(format!("{}-{sequence}.ppm", std::process::id()));
    let error_path = PathBuf::from(format!("{}.error", path.display()));
    let handle = state.renderers().freeze_scene(output, &path.display().to_string())?;
    let deadline = std::time::Instant::now() + timeout;
    let mut failure = None;
    let captured = loop {
        if path.metadata().is_ok_and(|metadata| metadata.len() > 16) {
            break true;
        }
        if error_path.is_file() {
            failure = std::fs::read_to_string(&error_path).ok();
            break false;
        }
        if !state.renderers().scene_freeze_alive(&handle) {
            failure = Some("renderer exited or was replaced".into());
            break false;
        }
        if std::time::Instant::now() >= deadline {
            failure = Some(format!("timed out after {} ms", timeout.as_millis()));
            break false;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    state.renderers().finish_scene_freeze(&handle);
    if !captured {
        let detail = failure.unwrap_or_else(|| "capture failed".into());
        log::warn!("scene handoff: live frame capture failed for {output} ({detail})");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&error_path);
        return None;
    }
    log::info!(
        "scene handoff: captured live frame for {output} from renderer {} ({})",
        handle.key,
        handle.pid
    );
    let cleanup_path = path.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(30));
        let _ = std::fs::remove_file(cleanup_path);
    });
    Some(path.display().to_string())
}

fn previous_media(state: &WallState, outputs: &[String]) -> Option<String> {
    if let Some(output) = outputs.first()
        && let Some(frame) = capture_transition_frame(state, output)
    {
        return Some(frame);
    }
    let assignments = state.renderers().assignments();
    let previous = outputs
        .iter()
        .find_map(|output| assignments.get(output))
        .or_else(|| assignments.get("*"))?;
    let path = Path::new(previous);
    if path.is_file() {
        return Some(previous.clone());
    }
    if path.is_dir() {
        return find_preview(path).map(|preview| preview.display().to_string());
    }
    valid_we_id(previous)
        .then(|| state.config().we_dir().join(previous))
        .and_then(|directory| find_preview(&directory))
        .map(|preview| preview.display().to_string())
}

fn native_scene<'a>(
    state: &'a WallState,
    outputs: &[String],
    item_dir: &std::path::Path,
    we_id: &str,
    properties: &serde_json::Map<String, serde_json::Value>,
    mute: bool,
    volume: u32,
    allow_warm_swap: bool,
) -> anyhow::Result<NativeSceneCandidate<'a>> {
    if !["scene.pkg", "gifscene.pkg"].iter().any(|name| item_dir.join(name).is_file()) {
        anyhow::bail!("native Wallpaper Engine scene package is missing in {}", item_dir.display());
    }
    let dir = item_dir.display().to_string();
    let fill = state.config().renderer().we_scene_fill_mode();
    let transitions = state.config().transition().active() && !state.apply().no_transition();
    let shader = state.config().transition().shader();
    let duration_ms = state.config().transition().duration_ms();
    let target = scene_renderer_key(outputs);
    let renderer_key = target.clone();
    if allow_warm_swap
        && apply::native_scene_policy_matches(state)
        && state.renderers().is_scene_paper(&renderer_key)
        && state.renderers().has_video_paper(&renderer_key)
    {
        let pid = state.renderers().video_paper_pid(&renderer_key);
        if let Some(pid) = pid {
            state.renderers().arm_ready_gate(pid);
        }
        let overrides = (!properties.is_empty()).then_some(properties);
        let swapped = if transitions {
            state.renderers().scene_swap_fade(
                &renderer_key,
                &dir,
                &shader,
                duration_ms,
                mute,
                volume,
                overrides,
            )
        } else {
            state.renderers().scene_swap(&renderer_key, &dir, mute, volume, overrides)
        };
        if swapped {
            if pid.is_some_and(|pid| {
                state.renderers().wait_ready(pid, apply::NATIVE_SCENE_READY_TIMEOUT)
            }) {
                log::info!("we scene: warm swap via running native renderer");
                return Ok(NativeSceneCandidate {
                    key: renderer_key,
                    renderer: None,
                    properties: (we_id.to_string(), properties.clone()),
                });
            }
            anyhow::bail!(
                "native Wallpaper Engine renderer rejected or timed out during warm swap"
            );
        }
    }
    let mut args =
        vec![target, dir.clone(), "--scene".to_string(), dir, "--fill-mode".to_string(), fill];
    if !properties.is_empty()
        && let Ok(encoded) = serde_json::to_string(properties)
    {
        args.push("--scene-properties".to_string());
        args.push(encoded);
    }
    args.extend([
        "--mute".to_string(),
        mute.to_string(),
        "--volume".to_string(),
        volume.min(100).to_string(),
    ]);
    if transitions && let Some(from) = previous_media(state, outputs) {
        args.push("--transition-from".to_string());
        args.push(from);
        args.push("--shader".to_string());
        args.push(shader.clone());
        args.push("--duration-ms".to_string());
        args.push(duration_ms.to_string());
    }
    let renderer = apply::spawn_native_scene(state, &renderer_key, &args)?.wait_ready()?;
    Ok(NativeSceneCandidate {
        key: renderer_key,
        renderer: Some(renderer),
        properties: (we_id.to_string(), properties.clone()),
    })
}

pub fn apply_we(state: &WallState, we_id: &str) -> anyhow::Result<Option<String>> {
    if !state.config().steam_enabled() {
        anyhow::bail!("steam/WE feature is disabled");
    }
    if !valid_we_id(we_id) {
        anyhow::bail!("invalid WE id: {we_id}");
    }
    let item_dir = state.config().we_dir().join(we_id);
    if !item_dir.is_dir() {
        anyhow::bail!("WE item not found: {}", item_dir.display());
    }
    let (ty, file) = read_project_type(&item_dir);
    if !is_supported_project(&ty) {
        anyhow::bail!("WE item {we_id} has unsupported type {ty:?}");
    }
    let preview = find_preview(&item_dir).map(|path| path.display().to_string());

    crate::awww::stop();
    let (mute, volume) = {
        let cfg = state.config();
        crate::audio::resolve_defaults(
            &cfg.cache_dir(),
            "*",
            cfg.renderer().mute(),
            cfg.renderer().volume(),
        )
    };
    if ty == "video" {
        if file.is_empty() {
            anyhow::bail!("WE video item {we_id} has no media file in project.json");
        }
        let Some(video) = safe_item_join(&item_dir, &file) else {
            anyhow::bail!("WE item has unsafe video file path: {file}");
        };
        apply::apply_video(
            state,
            "*",
            &video.display().to_string(),
            &state.config().display().fill_mode(),
            mute,
            volume,
        )?;
    } else {
        let outs = outputs::names();
        let cache = state.config().cache_dir();
        let prev = crate::audio::read_state(&cache);
        let mut map = serde_json::Map::new();
        let keys: Vec<String> = if outs.is_empty() { vec!["*".to_string()] } else { outs.clone() };
        for out in &keys {
            let (out_mute, out_vol) = crate::audio::carried_audio(&prev, out, mute, volume);
            map.insert(
                out.clone(),
                crate::audio::entry(wall_proto::kind::WE, "", we_id, out_mute, out_vol),
            );
        }
        let (groups, we_audio) = apply::resolve_we_from_state(&map);
        if crate::plasma::available() {
            crate::audio::write_state(&cache, &serde_json::Value::Object(map));
            crate::plasma::apply_current(state)?;
            crate::plasma::retire_native(state);
            for out in &keys {
                state.renderers().set_assignment(out, &item_dir.display().to_string());
            }
            return Ok(preview);
        }
        let renderer_key = scene_renderer_key(&outs);
        let (scene_mute, scene_volume) = we_audio.get(we_id).copied().unwrap_or((true, 100));
        let candidate = spawn_scene_for(state, &outs, we_id, scene_mute, scene_volume, true)?;
        commit_scene_set(state, vec![candidate])?;
        for renderer in state.renderers().take_video_papers_except(&[renderer_key]) {
            kill_held_renderer(renderer);
        }
        log::info!("we scene {we_id}: native renderer");
        state.renderers().set_we_render(groups, we_audio);
        state.renderers().kill_paper();
        state.renderers().defer_kill_stills(400);
        crate::audio::write_state(&cache, &serde_json::Value::Object(map));
        for out in &keys {
            state.renderers().set_assignment(out, &item_dir.display().to_string());
        }
    }

    Ok(preview)
}

#[path = "tests.rs"]
mod tests;
