#![cfg(test)]

use super::launch::{
    NativeScenePolicy, PERF_SCENE_EFFECT_CHAINS, PERF_SCENE_EFFECT_PASSES, PERF_SCENE_FPS,
    PERF_SCENE_MAX_DIMENSION, apply_native_scene_policy, current_native_scene_policy,
    native_scene_policy,
};
use super::lifecycle::spawn_base_still;
use super::policy::{
    NATIVE_SCENE_POLICY_KEY, PAPER_POLICY_KEY, current_paper_policy, native_scene_policy_matches,
    record_native_scene_policies, record_paper_policy,
};
use super::reconcile::{ReconcileIntent, reconcile_outputs};
use super::resolver::reconcile_targets;
use super::static_media::apply_static_per_output_transition;
use super::transition::TransitionSelection;
use super::wallpaper_engine::rebuild_we;
use super::*;
use crate::WallState;
use crate::domain::wallpaper::{
    is_video_path, managed_transition_args, still_args, transition_args, transition_args_for,
    transition_reveal_delay_ms, video_transition_args, vk_video_args,
};
use std::process::Command;

#[test]
fn resolve_image_sibling() {
    let tmp = tempfile::tempdir().unwrap();
    let jpg = tmp.path().join("bing-daily1.jpg");
    let webp = tmp.path().join("bing-daily1.webp");

    std::fs::write(&jpg, b"jpg").unwrap();
    assert_eq!(resolve_current_image(jpg.to_str().unwrap()), jpg.to_string_lossy(),);

    std::fs::remove_file(&jpg).unwrap();
    std::fs::write(&webp, b"webp").unwrap();
    assert_eq!(resolve_current_image(jpg.to_str().unwrap()), webp.to_string_lossy(),);

    let orphan = tmp.path().join("bing-gone.jpg");
    assert_eq!(resolve_current_image(orphan.to_str().unwrap()), orphan.to_string_lossy(),);

    let mp4 = tmp.path().join("clip.mp4");
    let webm = tmp.path().join("clip.webm");
    std::fs::write(&webm, b"webm").unwrap();
    assert_eq!(resolve_current_video(mp4.to_str().unwrap()), webm.to_string_lossy(),);
    let mp4b = tmp.path().join("clip2.mp4");
    std::fs::write(tmp.path().join("clip2.png"), b"png").unwrap();
    assert_eq!(resolve_current_video(mp4b.to_str().unwrap()), mp4b.to_string_lossy(),);
}

#[test]
fn dashy_paths_refused() {
    let st = Stub::new();
    assert!(spawn_base_still(&st, "*", "-rf", "fill").is_err(),);
    assert!(apply_video(&st, "*", "-oevil;mute=no", "fill", true, 80).is_err(),);
    assert!(apply_static_transition(&st, "-x.png", "/w/ok.png", "fill", "fade", 600).is_err(),);
    assert!(
        apply_video_transition(&st, "/v/ok.mp4", "-evil.mp4", "fill", "fade", 600, true, 80)
            .is_err()
    );
    assert!(settled_spawns(st.path()).is_empty());
    assert!(spawn_base_still(&st, "*", "/w/ok.png", "fill").is_ok(),);
}

#[test]
fn reconcile_respawns_crashed() {
    let st = Stub::new();
    let (dead, dead_stdin) = crate::infrastructure::renderers::exited_child();
    st.renderers().set_video_paper("DP-1", dead, dead_stdin);
    st.renderers().set_assignment("DP-1", "/v/a.mp4");
    seed(&st, "DP-1", "video", "/v/a.mp4", "", true, 50);
    reconcile_ready(&st, "fill", &["DP-1".to_string()], false, "fade", 600).unwrap();
    assert_eq!(
        wait_spawns(st.path(), 1),
        vec![vk_video_args("DP-1", "/v/a.mp4", "fill", true, 50)],
    );
    assert!(st.renderers().has_video_paper("DP-1"));
}

#[test]
fn is_video_path_gates() {
    for path in ["/w/clip.mp4", "/w/CLIP.MKV", "a.webm", "b.mov", "c.avi", "d.m4v"] {
        assert!(is_video_path(path),);
    }
    for path in ["/w/img.webp", "/w/pic.png", "photo.jpg", ""] {
        assert!(!is_video_path(path),);
    }
}

#[test]
fn resolve_we_groups() {
    let st = serde_json::json!({
        "DP-1": {"type": "we", "we_id": "111", "mute": false, "volume": 70},
        "DP-3": {"type": "we", "we_id": "111", "mute": true, "volume": 100},
        "DP-2": {"type": "video", "path": "/v/x.mp4", "mute": true, "volume": 50},
    });
    let (groups, audio) = resolve_we_from_state(st.as_object().unwrap());
    assert_eq!(groups.len(), 1);
    assert_eq!(groups.get("111"), Some(&vec!["DP-1".to_string(), "DP-3".to_string()]),);
    assert_eq!(audio.get("111"), Some(&(false, 70)));
}

#[test]
fn resolve_we_all_muted() {
    let st = serde_json::json!({
        "DP-1": {"type": "we", "we_id": "9", "mute": true, "volume": 40},
    });
    let (groups, audio) = resolve_we_from_state(st.as_object().unwrap());
    assert_eq!(groups.get("9"), Some(&vec!["DP-1".to_string()]));
    assert_eq!(audio.get("9"), Some(&(true, 100)));
}

#[test]
fn distinct_we_audio() {
    let st = serde_json::json!({
        "DP-1": {"type": "we", "we_id": "100", "mute": false, "volume": 29},
        "DP-2": {"type": "we", "we_id": "200", "mute": true, "volume": 0},
        "DP-3": {"type": "we", "we_id": "200", "mute": true, "volume": 0},
    });
    let (groups, audio) = resolve_we_from_state(st.as_object().unwrap());
    assert_eq!(groups.get("100"), Some(&vec!["DP-1".to_string()]));
    assert_eq!(groups.get("200"), Some(&vec!["DP-2".to_string(), "DP-3".to_string()]));
    assert_eq!(audio.get("100"), Some(&(false, 29)));
    assert_eq!(audio.get("200"), Some(&(true, 100)));
}

#[test]
fn distinct_we_launch_audio() {
    let dir = tempfile::tempdir().unwrap();
    let workshop = dir.path().join("we");
    for id in ["100", "200"] {
        let item = workshop.join(id);
        std::fs::create_dir_all(&item).unwrap();
        std::fs::write(item.join("scene.pkg"), b"probe bypassed").unwrap();
    }
    let bin = dir.path().join("renderer");
    let root = dir.path().display();
    std::fs::write(
        &bin,
        format!(
            "#!/bin/sh\nt=\"{root}/$$.args.tmp\"\n: > \"$t\"\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> \"$t\"; done\nmv \"$t\" \"{root}/$$.args\"\nexec cat\n"
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&bin).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&bin, permissions).unwrap();
    let state = WallState::test_new(serde_json::json!({
        "paths": {
            "cache": dir.path().join("cache").display().to_string(),
            "paperVkBin": bin.display().to_string(),
            "steamWorkshop": workshop.display().to_string(),
        },
        "weRender": {"engine": "native"},
    }));
    let groups = std::collections::BTreeMap::from([
        ("100".to_string(), vec!["DP-1".to_string()]),
        ("200".to_string(), vec!["DP-2".to_string(), "DP-3".to_string()]),
    ]);
    let audio = std::collections::BTreeMap::from([
        ("100".to_string(), (false, 29)),
        ("200".to_string(), (true, 100)),
    ]);
    with_readiness(&state, || rebuild_we(&state, groups, audio)).unwrap();
    let launches: std::collections::BTreeMap<_, _> =
        settled_spawns(dir.path()).into_iter().map(|args| (args[0].clone(), args)).collect();
    assert!(launches["DP-1"].windows(2).any(|pair| pair == ["--mute", "false"]));
    assert!(launches["DP-1"].windows(2).any(|pair| pair == ["--volume", "29"]));
    assert!(launches["DP-2,DP-3"].windows(2).any(|pair| pair == ["--mute", "true"]));
    assert!(launches["DP-2,DP-3"].windows(2).any(|pair| pair == ["--volume", "100"]));
    state.renderers().kill_all();
}

#[test]
fn resolve_we_empty() {
    let st = serde_json::json!({ "DP-1": {"type": "static", "path": "/w/a.webp"} });
    let (groups, _) = resolve_we_from_state(st.as_object().unwrap());
    assert!(groups.is_empty());
}

#[test]
fn transition_args_named() {
    let args = transition_args_for("DP-2", "/old.png", "/new.png", "fill", "fade", 600);
    assert_eq!(args[0], "DP-2");
    assert_eq!(args[1], "/new.png");
    assert!(args.windows(2).any(|pair| pair == ["--transition-from", "/old.png"]));
    assert!(args.windows(2).any(|pair| pair == ["--duration-ms", "600"]));
    assert!(args.windows(2).any(|pair| pair == ["--layer", "bottom"]));
    assert!(!args.contains(&"--persist".to_string()));
}

fn standalone(mut arguments: Vec<String>) -> Vec<String> {
    arguments.push("--standalone".to_string());
    arguments
}

fn staged(mut arguments: Vec<String>) -> Vec<String> {
    arguments.push("--transition-hold".to_string());
    arguments
}

#[test]
fn video_transition_named() {
    let args =
        video_transition_args("DP-1", "/old.mp4", "/new.mp4", "fill", "fade", 600, false, 70);
    assert_eq!(args[0], "DP-1");
    assert_eq!(args[1], "/new.mp4");
    assert!(args.windows(2).any(|pair| pair == ["--transition-from", "/old.mp4"]));
    assert!(args.contains(&"--persist".to_string()),);
}

#[test]
fn targets_include_missing() {
    let mut map = serde_json::Map::new();
    map.insert("DP-1".into(), serde_json::json!({"type": "static"}));
    map.insert("DP-2".into(), serde_json::json!({"type": "static"}));
    map.insert("DP-3".into(), serde_json::json!({"type": "video"}));
    let monitors = vec!["DP-1".to_string(), "DP-2".to_string()];
    let targets = reconcile_targets(&monitors, &map);
    assert!(targets.contains(&"DP-3".to_string()),);
    assert_eq!(targets.len(), 3);
}

#[test]
fn targets_skip_wildcard() {
    let mut map = serde_json::Map::new();
    map.insert("*".into(), serde_json::json!({"type": "video"}));
    map.insert("DP-1".into(), serde_json::json!({"type": "static"}));
    let monitors = vec!["DP-1".to_string(), "DP-2".to_string()];
    let targets = reconcile_targets(&monitors, &map);
    assert!(!targets.contains(&"*".to_string()));
    assert_eq!(targets, vec!["DP-1".to_string(), "DP-2".to_string()]);
}

#[test]
fn reveal_delay_trails() {
    assert_eq!(transition_reveal_delay_ms(600), 450);
    assert_eq!(transition_reveal_delay_ms(800), 600);
    assert!(transition_reveal_delay_ms(300) < 300);
    assert!(transition_reveal_delay_ms(120) <= 120);
}

#[test]
fn still_args_star() {
    assert_eq!(still_args("*", "/w.png", "fill"), vec!["*", "/w.png", "--fill-mode", "fill"]);
}

#[test]
fn still_args_named() {
    assert_eq!(still_args("DP-1", "/w.png", "fill"), vec!["DP-1", "/w.png", "--fill-mode", "fill"]);
}

#[test]
fn transition_args_flags() {
    let args = transition_args("/old.png", "/new.png", "fill", "fade", 800);
    assert_eq!(args[0], "*");
    assert_eq!(args[1], "/new.png");
    assert!(args.windows(2).any(|pair| pair == ["--transition-from", "/old.png"]));
    assert!(args.windows(2).any(|pair| pair == ["--shader", "fade"]));
    assert!(args.windows(2).any(|pair| pair == ["--duration-ms", "800"]));
    assert!(args.windows(2).any(|pair| pair == ["--layer", "bottom"]));
    assert!(!args.contains(&"--persist".to_string()));
}

#[test]
fn managed_transition_persists() {
    let args = managed_transition_args("/old.png", "/new.png", "fill", "fade", 400);
    assert!(args.contains(&"--persist".to_string()));
    assert!(args.windows(2).any(|pair| pair == ["--duration-ms", "400"]));
}

#[test]
fn vk_args_audio() {
    assert_eq!(
        vk_video_args("DP-3", "/v.mp4", "fill", true, 80),
        vec!["DP-3", "/v.mp4", "--fill-mode", "fill", "-o", "mute=yes;volume=80"]
    );
    assert_eq!(
        vk_video_args("*", "/v.mp4", "fit", false, 55),
        vec!["*", "/v.mp4", "--fill-mode", "fit", "-o", "mute=no;volume=55"]
    );
}

use std::ops::Deref;
use std::path::Path;
use std::process::Child;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use crate::infrastructure::renderers::capture_child;

struct Stub {
    dir: tempfile::TempDir,
    state: Arc<WallState>,
}

impl Stub {
    fn new() -> Self {
        Self::with_display(&serde_json::json!({}))
    }

    fn with_display(display: &serde_json::Value) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(stub_state_with_display(dir.path(), display));
        Self { dir, state }
    }

    fn with_video_multi(video_multi: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(stub_state(dir.path(), &serde_json::json!({}), video_multi));
        Self { dir, state }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn readiness(&self) -> Readiness {
        Readiness::new(self.state.clone())
    }
}

impl Deref for Stub {
    type Target = WallState;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.renderers().kill_all();
    }
}

fn stub_state_with_display(dir: &Path, display: &serde_json::Value) -> WallState {
    stub_state(dir, display, true)
}

fn stub_state(dir: &Path, display: &serde_json::Value, video_multi: bool) -> WallState {
    let bin = dir.join("stub-renderer");
    let root = dir.display();
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$$\" >> \"{root}/launch-order\"\nt=\"{root}/$$.args.tmp\"\n: > \"$t\"\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> \"$t\"; done\nmv \"$t\" \"{root}/$$.args\"\nexec cat > \"{root}/$$.stdin\"\n"
    );
    std::fs::write(&bin, script).unwrap();
    let mut perm = std::fs::metadata(&bin).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
    std::fs::set_permissions(&bin, perm).unwrap();
    let cache = dir.join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    WallState::test_new(serde_json::json!({
        "display": display,
        "paper": {"videoEngine": "vulkan", "videoMultiProcess": video_multi},
        "paths": {
            "cache": cache.display().to_string(),
            "paperStillBin": bin.display().to_string(),
            "paperVkBin": bin.display().to_string(),
            "steamWorkshop": dir.join("we").display().to_string(),
        }
    }))
}

fn sleeper() -> Child {
    Command::new("sleep").arg("60").spawn().expect("spawn sleep")
}

#[test]
fn paper_policy_gate_stale() {
    let st = Stub::new();
    assert!(paper_policy_matches(&st));
    st.renderers().set_policy(PAPER_POLICY_KEY, &current_paper_policy(&st).signature());
    assert!(paper_policy_matches(&st));
    st.renderers().set_policy(PAPER_POLICY_KEY, "stale");
    assert!(!paper_policy_matches(&st));
}

#[test]
fn active_policy_gate_video() {
    let st = Stub::new();
    seed(&st, "*", "video", "/v/a.mp4", "", true, 80);
    record_paper_policy(&st);
    assert!(active_renderer_policy_matches(&st));

    st.renderers().set_policy(PAPER_POLICY_KEY, "stale");
    assert!(!active_renderer_policy_matches(&st));
}

#[test]
fn paper_policy_signature_fields() {
    let st = Stub::new();
    let base = current_paper_policy(&st);
    let mut changed = base.clone();
    changed.idle_pause_seconds = base.idle_pause_seconds.saturating_add(1);
    assert_ne!(base.signature(), changed.signature());
    changed = base.clone();
    changed.video_engine = "tinier".to_string();
    assert_ne!(base.signature(), changed.signature());
    changed = base.clone();
    changed.fill_mode = "cover".to_string();
    assert_ne!(base.signature(), changed.signature());
    changed = base.clone();
    changed.sand_fps = "30".to_string();
    assert_ne!(base.signature(), changed.signature());
}

#[test]
fn paper_policy_binary_rebuild() {
    let st = Stub::new();
    record_paper_policy(&st);
    assert!(paper_policy_matches(&st));

    let bin = st.config().renderer().vk_bin();
    let replacement = st.path().join("replacement-renderer");
    std::fs::write(&replacement, b"#!/bin/sh\nexec cat\n# rebuilt\n").unwrap();
    std::fs::rename(&replacement, &bin).unwrap();

    assert!(!paper_policy_matches(&st));
}

#[test]
fn native_scene_policy_normal() {
    assert_eq!(
        native_scene_policy(75, false, false),
        NativeScenePolicy {
            fill_mode: String::new(),
            assets_dir: String::new(),
            fps: 75,
            disable_particles: false,
            max_dimension: None,
            effect_chains: None,
            effect_passes: None,
        }
    );
}

#[test]
fn native_scene_policy_performance() {
    assert_eq!(
        native_scene_policy(120, true, false),
        NativeScenePolicy {
            fill_mode: String::new(),
            assets_dir: String::new(),
            fps: PERF_SCENE_FPS,
            disable_particles: false,
            max_dimension: Some(PERF_SCENE_MAX_DIMENSION),
            effect_chains: Some(PERF_SCENE_EFFECT_CHAINS),
            effect_passes: Some(PERF_SCENE_EFFECT_PASSES),
        }
    );
    assert_eq!(native_scene_policy(24, true, false).fps, 24);
}

#[test]
fn native_scene_signature_fields() {
    assert_eq!(native_scene_policy(60, false, false).signature(), "v6:::60:false:0:0:0");
    assert_eq!(native_scene_policy(120, true, false).signature(), "v6:::30:false:2048:4:8");
    assert_ne!(
        native_scene_policy(60, false, false).signature(),
        native_scene_policy(30, false, false).signature()
    );
    assert_ne!(
        native_scene_policy(60, false, false).signature(),
        native_scene_policy(60, false, true).signature()
    );
}

#[test]
fn native_scene_policy_nests_paper() {
    let st = Stub::new();
    st.renderers()
        .set_policy(NATIVE_SCENE_POLICY_KEY, &current_native_scene_policy(&st).signature());
    assert!(native_scene_policy_matches(&st));
    st.renderers().set_policy(PAPER_POLICY_KEY, "stale");
    assert!(!native_scene_policy_matches(&st));
}

#[test]
fn native_only_scene_matches() {
    let st = Stub::new();
    st.renderers().set_video_paper("*", sleeper(), None);
    st.renderers().mark_scene_paper("*", true);
    record_native_scene_policies(&st);

    assert!(renderer_policy_matches(&st, wall_proto::kind::WE));
}

fn command_env(cmd: &Command, key: &str) -> Option<String> {
    cmd.get_envs()
        .find(|(name, _)| *name == std::ffi::OsStr::new(key))
        .and_then(|(_, value)| value)
        .map(|value| value.to_string_lossy().into_owned())
}

#[test]
fn native_scene_policy_env() {
    let mut normal = Command::new("unused");
    apply_native_scene_policy(&mut normal, &native_scene_policy(75, false, false));
    assert_eq!(command_env(&normal, "SKWD_PAPER_WE_FPS").as_deref(), Some("75"));
    assert_eq!(command_env(&normal, "SKWD_VK_SCENE_MAX"), None);
    assert_eq!(command_env(&normal, "SKWD_VK_SCENE_FX"), None);
    assert_eq!(command_env(&normal, "SKWD_VK_FX_PASSES"), None);
    assert_eq!(command_env(&normal, "SKWD_WE_ASSETS"), None);
    assert_eq!(command_env(&normal, "SKWD_PAPER_WE_DISABLE_PARTICLES").as_deref(), Some("0"));

    let mut with_assets = native_scene_policy(60, false, true);
    with_assets.assets_dir = "/opt/wallpaper_engine/assets".into();
    let mut configured = Command::new("unused");
    apply_native_scene_policy(&mut configured, &with_assets);
    assert_eq!(
        command_env(&configured, "SKWD_WE_ASSETS").as_deref(),
        Some("/opt/wallpaper_engine/assets")
    );
    assert_eq!(command_env(&configured, "SKWD_PAPER_WE_DISABLE_PARTICLES").as_deref(), Some("1"));

    let mut performance = Command::new("unused");
    apply_native_scene_policy(&mut performance, &native_scene_policy(120, true, false));
    assert_eq!(command_env(&performance, "SKWD_PAPER_WE_FPS").as_deref(), Some("30"));
    assert_eq!(command_env(&performance, "SKWD_VK_SCENE_MAX").as_deref(), Some("2048"));
    assert_eq!(command_env(&performance, "SKWD_VK_SCENE_FX").as_deref(), Some("4"));
    assert_eq!(command_env(&performance, "SKWD_VK_FX_PASSES").as_deref(), Some("8"));
}

fn spawn_argvs(dir: &Path) -> Vec<Vec<String>> {
    let mut argvs: Vec<Vec<String>> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "args"))
        .map(|entry| {
            std::fs::read_to_string(entry.path()).unwrap().lines().map(str::to_string).collect()
        })
        .collect();
    argvs.sort();
    argvs
}

fn wait_spawns(dir: &Path, count: usize) -> Vec<Vec<String>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let argvs = spawn_argvs(dir);
        if argvs.len() >= count {
            return argvs;
        }
        assert!(Instant::now() < deadline, "{count} spawns, got {argvs:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn settled_spawns(dir: &Path) -> Vec<Vec<String>> {
    std::thread::sleep(Duration::from_millis(150));
    spawn_argvs(dir)
}

fn launch_order(dir: &Path) -> Vec<(u32, Vec<String>)> {
    std::fs::read_to_string(dir.join("launch-order"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.parse::<u32>().ok())
        .filter_map(|pid| {
            let path = dir.join(format!("{pid}.args"));
            std::fs::read_to_string(path)
                .ok()
                .map(|args| (pid, args.lines().map(str::to_string).collect::<Vec<String>>()))
        })
        .collect()
}

fn signal_stub_spawns(st: &WallState, dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        if entry.path().extension().is_some_and(|ext| ext == "args")
            && let Some(pid) = entry
                .path()
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.parse().ok())
        {
            st.renderers().signal_ready(pid);
        }
    }
}

fn wait_stdin_lines(path: &Path, count: usize) -> Vec<serde_json::Value> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let lines: Vec<serde_json::Value> = std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        if lines.len() >= count {
            return lines;
        }
        assert!(Instant::now() < deadline, "{count} lines at {path:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn seed(st: &WallState, out: &str, ty: &str, path: &str, we: &str, mute: bool, vol: u32) {
    crate::audio::set_entry(&st.config().cache_dir(), out, ty, path, we, mute, vol);
}

fn recorded(st: &WallState) -> serde_json::Value {
    crate::audio::read_state(&st.config().cache_dir())
}

fn with_readiness<T>(st: &WallState, action: impl FnOnce() -> T) -> T {
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let waiter = scope.spawn(|| {
            while !stop.load(Ordering::Relaxed) {
                for pid in st.renderers().wallpaper_pids() {
                    st.renderers().signal_ready(pid);
                }
                if let Some(dir) = Path::new(&st.config().renderer().vk_bin()).parent() {
                    signal_stub_spawns(st, dir);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        let result = action();
        stop.store(true, Ordering::Relaxed);
        let _ = waiter.join();
        result
    })
}

fn with_recorded_readiness<T>(
    st: &WallState,
    incumbents: &[u32],
    action: impl FnOnce() -> T,
) -> (T, Vec<u32>) {
    let stop = AtomicBool::new(false);
    let staged = std::sync::Mutex::new(Vec::new());
    let result = std::thread::scope(|scope| {
        let waiter = scope.spawn(|| {
            while !stop.load(Ordering::Relaxed) {
                for pid in st.renderers().wallpaper_pids() {
                    if !incumbents.contains(&pid) {
                        let mut recorded = staged.lock().unwrap();
                        if !recorded.contains(&pid) {
                            recorded.push(pid);
                        }
                    }
                    st.renderers().signal_ready(pid);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        let result = action();
        stop.store(true, Ordering::Relaxed);
        waiter.join().unwrap();
        result
    });
    let staged = staged.into_inner().unwrap();
    (result, staged)
}

fn reconcile_ready(
    st: &WallState,
    _fill: &str,
    mons: &[String],
    trans: bool,
    shader: &str,
    dur: u64,
) -> anyhow::Result<()> {
    let transition =
        TransitionSelection::Explicit { enabled: trans, shader, duration_ms: dur }.resolve(st);
    with_readiness(st, || reconcile_outputs(st, mons, &ReconcileIntent::Apply { transition }))
}

fn refresh_policy_ready(st: &WallState) -> anyhow::Result<()> {
    with_readiness(st, || refresh_renderer_policy(st))
}

struct Readiness {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Readiness {
    fn new(st: Arc<WallState>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                for pid in st.renderers().wallpaper_pids() {
                    st.renderers().signal_ready(pid);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        Self { stop, thread: Some(thread) }
    }
}

impl Drop for Readiness {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[test]
fn reconcile_plain_video() {
    let st = Stub::new();
    seed(&st, "DP-1", "video", "/v/a.mp4", "", true, 50);
    reconcile_ready(&st, "fill", &["DP-1".to_string()], false, "fade", 600).unwrap();
    assert_eq!(
        wait_spawns(st.path(), 1),
        vec![vk_video_args("DP-1", "/v/a.mp4", "fill", true, 50)]
    );
    assert!(st.renderers().has_video_paper("DP-1"));
    assert_eq!(st.renderers().assignments().get("DP-1").map(String::as_str), Some("/v/a.mp4"));
}

#[test]
fn reconcile_video_fade() {
    let st = Stub::new();
    let old = st.path().join("old.png");
    std::fs::write(&old, b"image").unwrap();
    let old = old.to_str().unwrap();
    st.renderers().set_assignment("DP-1", old);
    seed(&st, "DP-1", "video", "/v/new.mp4", "", false, 70);
    reconcile_ready(&st, "fill", &["DP-1".to_string()], true, "fade", 600).unwrap();
    assert_eq!(
        wait_spawns(st.path(), 1),
        vec![video_transition_args("DP-1", old, "/v/new.mp4", "fill", "fade", 600, false, 70)]
    );
    assert_eq!(st.renderers().assignments().get("DP-1").map(String::as_str), Some("/v/new.mp4"));
}

#[test]
fn named_output_uses_request_transition_override() {
    let st = Stub::new();
    let old = st.path().join("old.png");
    std::fs::write(&old, b"image").unwrap();
    let old = old.to_str().unwrap();
    st.renderers().set_assignment("DP-1", old);

    with_readiness(&st, || {
        apply_output_with_transition(
            &st,
            "DP-1",
            wall_proto::kind::VIDEO,
            "/v/new.mp4",
            "",
            "fill",
            false,
            70,
            Some(crate::backend::wallpaper::OutputTransitionRequest {
                enabled: true,
                shader: "glitch",
                duration_ms: 180,
            }),
        )
    })
    .unwrap();

    assert_eq!(
        wait_spawns(st.path(), 1),
        vec![video_transition_args("DP-1", old, "/v/new.mp4", "fill", "glitch", 180, false, 70,)]
    );
}

#[test]
fn first_video_cold_start() {
    let st = Stub::new();
    seed(&st, "DP-1", "video", "/v/a.mp4", "", true, 100);
    reconcile_ready(&st, "fill", &["DP-1".to_string()], true, "fade", 600).unwrap();
    assert_eq!(
        wait_spawns(st.path(), 1),
        vec![vk_video_args("DP-1", "/v/a.mp4", "fill", true, 100)]
    );
}

#[test]
fn reconcile_swaps_live() {
    let st = Stub::new();
    let out = st.path().join("dp1-live.stdin");
    let (child, stdin) = capture_child(&out);
    st.renderers().set_video_paper("DP-1", child, stdin);
    st.renderers().set_assignment("DP-1", "/v/old.mp4");
    seed(&st, "DP-1", "video", "/v/new.mp4", "", false, 70);
    reconcile_ready(&st, "fill", &["DP-1".to_string()], true, "fade", 600).unwrap();
    assert!(settled_spawns(st.path()).is_empty(),);
    assert!(st.renderers().has_video_paper("DP-1"));
    assert_eq!(
        wait_stdin_lines(&out, 1),
        vec![serde_json::json!({
            "to": "/v/new.mp4",
            "shader": "fade",
            "duration_ms": 600,
            "mute": false,
            "volume": 70
        })]
    );
    assert_eq!(st.renderers().assignments().get("DP-1").map(String::as_str), Some("/v/new.mp4"));
}

#[test]
fn reconcile_keeps_unchanged() {
    let st = Stub::new();
    let keep_out = st.path().join("dp1-live.stdin");
    let (child, stdin) = capture_child(&keep_out);
    st.renderers().set_video_paper("DP-1", child, stdin);
    st.renderers().set_assignment("DP-1", "/v/a.mp4");
    st.renderers().set_video_paper("DP-2", sleeper(), None);
    st.renderers().set_assignment("DP-2", "/v/b.mp4");
    seed(&st, "DP-1", "video", "/v/a.mp4", "", true, 100);
    seed(&st, "DP-2", "static", "/w/c.png", "", true, 0);
    let mons = vec!["DP-1".to_string(), "DP-2".to_string()];
    reconcile_ready(&st, "fill", &mons, false, "fade", 600).unwrap();
    wait_spawns(st.path(), 1);
    assert_eq!(
        settled_spawns(st.path()),
        vec![vec!["DP-2", "/w/c.png", "--fill-mode", "fill", "--persist"]]
    );
    assert!(st.renderers().has_video_paper("DP-1"));
    assert!(!st.renderers().has_video_paper("DP-2"));
    assert!(st.renderers().has_output_still("DP-2"));
    let (mut child, stdin) = st.renderers().take_video_paper("DP-1").unwrap();
    drop(stdin);
    let _ = child.wait();
    assert_eq!(std::fs::read_to_string(&keep_out).unwrap(), "",);
}

#[test]
fn reconcile_static_overlay() {
    let st = Stub::with_display(&serde_json::json!({
        "fillMode": "fill",
        "fillModes": {"DP-1": "fit"}
    }));
    let out = st.path().join("dp1-still.stdin");
    let (child, stdin) = capture_child(&out);
    let old = st.path().join("old.png");
    std::fs::write(&old, b"image").unwrap();
    let old = old.to_str().unwrap();
    st.renderers().set_output_still("DP-1", child, stdin);
    st.renderers().set_assignment("DP-1", old);
    seed(&st, "DP-1", "static", "/w/new.png", "", true, 0);
    reconcile_ready(&st, "fill", &["DP-1".to_string()], true, "fade", 600).unwrap();
    assert_eq!(
        wait_spawns(st.path(), 1),
        vec![staged(transition_args_for("DP-1", old, "/w/new.png", "fit", "fade", 600,))]
    );
    let overlay = launch_order(st.path()).into_iter().next().unwrap();
    assert_eq!(
        wait_stdin_lines(&st.path().join(format!("{}.stdin", overlay.0)), 1)[0]["pause"],
        false
    );
    assert!(st.renderers().has_output_still("DP-1"));
    assert_eq!(
        wait_stdin_lines(&out, 1),
        vec![serde_json::json!({"path": "/w/new.png", "fill": "fit"})]
    );
}

#[test]
fn shared_still_transitions_changed() {
    let st = Stub::new();
    let old = st.path().join("old.png");
    let shared = st.path().join("shared.png");
    std::fs::write(&old, b"old").unwrap();
    std::fs::write(&shared, b"shared").unwrap();
    let old = old.to_str().unwrap();
    let shared = shared.to_str().unwrap();
    let (dp1, dp1_stdin) = capture_child(&st.path().join("dp1-still.stdin"));
    let (dp2, dp2_stdin) = capture_child(&st.path().join("dp2-still.stdin"));
    st.renderers().set_output_still("DP-1", dp1, dp1_stdin);
    st.renderers().set_output_still("DP-2", dp2, dp2_stdin);
    st.renderers().set_assignment("DP-1", old);
    st.renderers().set_assignment("DP-2", shared);
    seed(&st, "DP-1", "static", shared, "", true, 0);
    seed(&st, "DP-2", "static", shared, "", true, 0);

    reconcile_ready(&st, "fill", &["DP-1".to_string(), "DP-2".to_string()], true, "fade", 600)
        .unwrap();

    let spawns = wait_spawns(st.path(), 2);
    assert!(
        spawns.contains(&staged(transition_args_for("DP-1", old, shared, "fill", "fade", 600,)))
    );
    assert!(spawns.contains(&vec![
        "DP-1,DP-2".to_string(),
        shared.to_string(),
        "--fill-mode".to_string(),
        "fill".to_string(),
        "--persist".to_string(),
    ]));
    assert!(
        !spawns.contains(&standalone(transition_args_for(
            "DP-2", shared, shared, "fill", "fade", 600,
        )))
    );
    assert!(st.renderers().has_output_still("DP-1,DP-2"));
    let launches = launch_order(st.path());
    assert!(launches[0].1.contains(&"--transition-hold".to_string()));
    assert!(launches[1].1.contains(&"--persist".to_string()));
    assert_eq!(
        wait_stdin_lines(&st.path().join(format!("{}.stdin", launches[0].0)), 1)[0]["pause"],
        false
    );
    assert_eq!(st.renderers().assignments().get("DP-1").map(String::as_str), Some(shared));
    assert_eq!(st.renderers().assignments().get("DP-2").map(String::as_str), Some(shared));
}

#[test]
fn video_to_static_stages_overlay_before_background() {
    let st = Stub::new();
    let old = st.path().join("old.mp4");
    std::fs::write(&old, b"video").unwrap();
    let old = old.to_str().unwrap();
    let (video, video_stdin) = capture_child(&st.path().join("video.stdin"));
    st.renderers().set_video_paper("DP-1", video, video_stdin);
    st.renderers().set_assignment("DP-1", old);
    seed(&st, "DP-1", "static", "/w/new.png", "", true, 0);

    reconcile_ready(&st, "fill", &["DP-1".to_string()], true, "fade", 600).unwrap();

    let launches = launch_order(st.path());
    assert_eq!(launches.len(), 2);
    assert_eq!(
        launches[0].1,
        staged(transition_args_for("DP-1", old, "/w/new.png", "fill", "fade", 600))
    );
    assert_eq!(launches[1].1, vec!["DP-1", "/w/new.png", "--fill-mode", "fill", "--persist"]);
    assert_eq!(
        wait_stdin_lines(&st.path().join(format!("{}.stdin", launches[0].0)), 1)[0]["pause"],
        false
    );
    assert!(st.renderers().has_output_still("DP-1"));
    assert!(!st.renderers().has_video_paper("DP-1"));
}

#[test]
fn mixed_fill_per_output_overlays() {
    let st = Stub::with_display(&serde_json::json!({
        "fillMode": "fill",
        "fillModes": {"DP-2": "fit"}
    }));
    let dp1_out = st.path().join("dp1-still.stdin");
    let dp2_out = st.path().join("dp2-still.stdin");
    let (dp1, dp1_stdin) = capture_child(&dp1_out);
    let (dp2, dp2_stdin) = capture_child(&dp2_out);
    st.renderers().set_output_still("DP-1", dp1, dp1_stdin);
    st.renderers().set_output_still("DP-2", dp2, dp2_stdin);
    st.renderers().set_assignment("DP-1", "/w/old.png");
    st.renderers().set_assignment("DP-2", "/w/old.png");
    let outputs = vec!["DP-1".to_string(), "DP-2".to_string()];

    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let stop_ref = &stop;
        let ready = scope.spawn(|| {
            while !stop_ref.load(Ordering::Relaxed) {
                signal_stub_spawns(&st, st.path());
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        apply_static_per_output_transition(
            &st,
            &outputs,
            "/w/old.png",
            "/w/new.png",
            "fill",
            "fade",
            600,
            None,
        )
        .unwrap();
        stop_ref.store(true, Ordering::Relaxed);
        ready.join().unwrap();
    });

    let spawns = wait_spawns(st.path(), 2);
    assert!(spawns.contains(&standalone(transition_args_for(
        "DP-1",
        "/w/old.png",
        "/w/new.png",
        "fill",
        "fade",
        600,
    ))));
    assert!(spawns.contains(&standalone(transition_args_for(
        "DP-2",
        "/w/old.png",
        "/w/new.png",
        "fit",
        "fade",
        600,
    ))));
    assert_eq!(
        wait_stdin_lines(&dp1_out, 1),
        vec![serde_json::json!({"path": "/w/new.png", "fill": "fill"})]
    );
    assert_eq!(
        wait_stdin_lines(&dp2_out, 1),
        vec![serde_json::json!({"path": "/w/new.png", "fill": "fit"})]
    );
    assert_eq!(recorded(&st)["DP-1"]["path"], "/w/new.png");
    assert_eq!(recorded(&st)["DP-2"]["path"], "/w/new.png");
}

#[test]
fn unchanged_static_noop() {
    let st = Stub::new();
    let out = st.path().join("dp1-still.stdin");
    let (child, stdin) = capture_child(&out);
    st.renderers().set_output_still("DP-1", child, stdin);
    st.renderers().set_assignment("DP-1", "/w/a.png");
    seed(&st, "DP-1", "static", "/w/a.png", "", true, 0);
    reconcile_ready(&st, "fill", &["DP-1".to_string()], true, "fade", 600).unwrap();
    assert!(settled_spawns(st.path()).is_empty());
    assert!(st.renderers().has_output_still("DP-1"));
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "");
}

#[test]
fn reconcile_skips_unknown() {
    let st = Stub::new();
    seed(&st, "DP-1", "plasma", "/x/y.z", "", true, 0);
    let mons = vec!["DP-1".to_string(), "DP-2".to_string()];
    reconcile_ready(&st, "fill", &mons, true, "fade", 600).unwrap();
    assert!(settled_spawns(st.path()).is_empty());
    assert!(st.renderers().assignments().is_empty());
}

#[test]
fn reconcile_replaces_star() {
    let st = Stub::new();
    st.renderers().set_base_still(sleeper(), None);
    st.renderers().set_video_paper("*", sleeper(), None);
    st.renderers().set_paper_stdin(None);
    st.renderers().swap_paper(sleeper());
    seed(&st, "DP-1", "static", "/w/a.png", "", true, 0);
    reconcile_ready(&st, "fill", &["DP-1".to_string()], false, "fade", 600).unwrap();
    assert_eq!(
        wait_spawns(st.path(), 1),
        vec![vec!["DP-1", "/w/a.png", "--fill-mode", "fill", "--persist"]]
    );
    assert!(!st.renderers().has_base_still());
    assert!(!st.renderers().has_video_paper("*"));
    assert!(st.renderers().take_paper().is_none());
}

#[test]
fn reconcile_vk_still_to_video() {
    let st = Stub::new();
    let old = st.path().join("old.png");
    std::fs::write(&old, b"image").unwrap();
    let old = old.to_str().unwrap();
    st.renderers().set_assignment("DP-1", old);
    seed(&st, "DP-1", "video", "/v/a.mp4", "", true, 80);
    reconcile_ready(&st, "fill", &["DP-1".to_string()], true, "fade", 600).unwrap();
    assert_eq!(
        wait_spawns(st.path(), 1),
        vec![video_transition_args("DP-1", old, "/v/a.mp4", "fill", "fade", 600, true, 80)]
    );
}

#[test]
fn reconcile_vk_reshape_plain_spawn() {
    let st = Stub::new();
    st.renderers().set_video_paper("*", sleeper(), None);
    st.renderers().set_assignment("DP-1", "/v/a.mp4");
    seed(&st, "DP-1", "video", "/v/a.mp4", "", true, 80);
    reconcile_ready(&st, "fill", &["DP-1".to_string()], true, "fade", 600).unwrap();
    assert_eq!(
        wait_spawns(st.path(), 1),
        vec![vk_video_args("DP-1", "/v/a.mp4", "fill", true, 80)]
    );
    assert!(!st.renderers().has_video_paper("*"));
}

#[test]
fn we_regroup_failure_rollback() {
    use std::sync::atomic::AtomicU32;

    let dir = tempfile::tempdir().unwrap();
    let workshop = dir.path().join("we");
    let item = workshop.join("a-new");
    std::fs::create_dir_all(&item).unwrap();
    std::fs::write(item.join("scene.pkg"), b"probe bypassed").unwrap();
    let bin = dir.path().join("renderer");
    std::fs::write(&bin, "#!/bin/sh\nexec cat\n").unwrap();
    let mut permissions = std::fs::metadata(&bin).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&bin, permissions).unwrap();
    let cache = dir.path().join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    let st = WallState::test_new(serde_json::json!({
        "paths": {
            "cache": cache.display().to_string(),
            "paperVkBin": bin.display().to_string(),
            "steamWorkshop": workshop.display().to_string(),
        },
        "weRender": {"engine": "native"},
        "transition": {"enabled": false},
    }));

    let old_stdin_path = dir.path().join("old.stdin");
    let (old, old_stdin) = capture_child(&old_stdin_path);
    let old_pid = old.id();
    let old_renderer_key = "DP-1,DP-2";
    st.renderers().set_video_paper(old_renderer_key, old, old_stdin);
    st.renderers().mark_scene_paper(old_renderer_key, true);
    st.renderers().set_assignment("DP-1", "old-scene");
    st.renderers().set_assignment("DP-2", "old-scene");
    let assignments_before = st.renderers().assignments();
    let mut old_groups = std::collections::BTreeMap::new();
    old_groups.insert("old-scene".to_string(), vec!["DP-1".to_string(), "DP-2".to_string()]);
    let old_audio = std::collections::BTreeMap::from([("old-scene".to_string(), (true, 100))]);
    st.renderers().set_we_render(old_groups.clone(), old_audio.clone());
    record_native_scene_policies(&st);
    let native_policy_before = st.renderers().policy(NATIVE_SCENE_POLICY_KEY);

    seed(&st, "DP-1", "we", "", "a-new", true, 100);
    seed(&st, "DP-2", "we", "", "z/../invalid", true, 100);
    let stop = AtomicBool::new(false);
    let staged_pid = AtomicU32::new(0);
    let result = std::thread::scope(|scope| {
        let ready = scope.spawn(|| {
            while !stop.load(Ordering::Relaxed) {
                for pid in st.renderers().wallpaper_pids() {
                    if pid != old_pid {
                        staged_pid.store(pid, Ordering::Relaxed);
                        st.renderers().signal_ready(pid);
                    }
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        let result = reconcile_outputs(
            &st,
            &["DP-1".to_string(), "DP-2".to_string()],
            &ReconcileIntent::PolicyRefresh,
        );
        stop.store(true, Ordering::Relaxed);
        ready.join().unwrap();
        result
    });

    assert!(result.unwrap_err().to_string().contains("invalid WE id"));
    let staged_pid = staged_pid.load(Ordering::Relaxed);
    assert_ne!(staged_pid, 0);
    assert_ne!(staged_pid, old_pid);
    assert!(!Path::new(&format!("/proc/{staged_pid}")).exists());
    assert_eq!(st.renderers().video_paper_pid(old_renderer_key), Some(old_pid));
    assert!(st.renderers().is_scene_paper(old_renderer_key));
    assert!(Path::new(&format!("/proc/{old_pid}")).exists());
    assert_eq!(st.renderers().assignments(), assignments_before);
    assert_eq!(st.renderers().policy(NATIVE_SCENE_POLICY_KEY), native_policy_before);
    assert!(st.renderers().we_render_matches(&old_groups, &old_audio));
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(std::fs::read(&old_stdin_path).unwrap_or_default(), b"");
    st.renderers().kill_all();
}

#[test]
fn mixed_shared_renderers_roll_back_when_we_prepare_fails() {
    let st = Stub::new();
    st.apply().set_no_transition(true);
    let valid_scene = st.path().join("we/a-new");
    std::fs::create_dir_all(&valid_scene).unwrap();
    std::fs::write(valid_scene.join("scene.pkg"), b"fixture").unwrap();

    let static_key = "DP-S1,DP-S2";
    let static_stdin_path = st.path().join("old-static.stdin");
    let (old_static, old_static_stdin) = capture_child(&static_stdin_path);
    let old_static_pid = old_static.id();
    st.renderers().restore_output_still(static_key, (old_static, old_static_stdin));

    let video_stdin_path = st.path().join("old-video.stdin");
    let (old_video, old_video_stdin) = capture_child(&video_stdin_path);
    let old_video_pid = old_video.id();
    st.renderers().set_video_paper("multi", old_video, old_video_stdin);

    let scene_key = "DP-W1,DP-W2";
    let scene_stdin_path = st.path().join("old-scene.stdin");
    let (old_scene, old_scene_stdin) = capture_child(&scene_stdin_path);
    let old_scene_pid = old_scene.id();
    st.renderers().set_video_paper(scene_key, old_scene, old_scene_stdin);
    st.renderers().mark_scene_paper(scene_key, true);

    for output in ["DP-S1", "DP-S2"] {
        st.renderers().set_assignment(output, "/w/old.png");
        seed(&st, output, "static", "/w/new.png", "", true, 100);
    }
    for (output, path) in [("DP-V1", "/v/new-a.mp4"), ("DP-V2", "/v/new-b.mp4")] {
        st.renderers().set_assignment(output, "/v/old.mp4");
        seed(&st, output, "video", path, "", true, 100);
    }
    st.renderers().set_assignment("multi", "old-multi-spec");
    for output in ["DP-W1", "DP-W2"] {
        st.renderers().set_assignment(output, "old-scene");
    }
    seed(&st, "DP-W1", "we", "", "a-new", true, 100);
    seed(&st, "DP-W2", "we", "", "z/../invalid", true, 100);
    let assignments_before = st.renderers().assignments();
    let old_groups = std::collections::BTreeMap::from([(
        "old-scene".to_string(),
        vec!["DP-W1".to_string(), "DP-W2".to_string()],
    )]);
    let old_audio = std::collections::BTreeMap::from([("old-scene".to_string(), (true, 100))]);
    st.renderers().set_we_render(old_groups.clone(), old_audio.clone());

    let monitors = ["DP-S1", "DP-S2", "DP-V1", "DP-V2", "DP-W1", "DP-W2"].map(str::to_string);
    let (result, staged_pids) =
        with_recorded_readiness(&st, &[old_static_pid, old_video_pid, old_scene_pid], || {
            reconcile_outputs(&st, &monitors, &ReconcileIntent::PolicyRefresh)
        });

    assert!(result.unwrap_err().to_string().contains("invalid WE id"));
    assert_eq!(staged_pids.len(), 3);
    for pid in staged_pids {
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }
    assert_eq!(st.renderers().video_paper_pid("multi"), Some(old_video_pid));
    assert_eq!(st.renderers().video_paper_pid(scene_key), Some(old_scene_pid));
    assert_eq!(
        st.renderers().take_output_still(static_key).map(|renderer| {
            let pid = renderer.0.id();
            st.renderers().restore_output_still(static_key, renderer);
            pid
        }),
        Some(old_static_pid)
    );
    assert!(st.renderers().is_scene_paper(scene_key));
    assert!(!st.renderers().is_scene_paper("DP-W1"));
    assert_eq!(st.renderers().assignments(), assignments_before);
    assert!(st.renderers().we_render_matches(&old_groups, &old_audio));
    for pid in [old_static_pid, old_video_pid, old_scene_pid] {
        assert!(Path::new(&format!("/proc/{pid}")).exists());
    }
}

#[test]
fn mixed_per_output_renderers_roll_back_when_we_prepare_fails() {
    let st = Stub::with_video_multi(false);
    let static_stdin_path = st.path().join("old-output-static.stdin");
    let (old_static, old_static_stdin) = capture_child(&static_stdin_path);
    let old_static_pid = old_static.id();
    st.renderers().set_output_still("DP-S", old_static, old_static_stdin);
    let video_stdin_path = st.path().join("old-output-video.stdin");
    let (old_video, old_video_stdin) = capture_child(&video_stdin_path);
    let old_video_pid = old_video.id();
    st.renderers().set_video_paper("DP-V", old_video, old_video_stdin);
    st.renderers().set_assignment("DP-S", "/w/old.png");
    st.renderers().set_assignment("DP-V", "/v/old.mp4");
    st.renderers().set_assignment("DP-W", "old-scene");
    let assignments_before = st.renderers().assignments();
    seed(&st, "DP-S", "static", "/w/new.png", "", true, 100);
    seed(&st, "DP-V", "video", "/v/new.mp4", "", true, 100);
    seed(&st, "DP-W", "we", "", "z/../invalid", true, 100);

    let monitors = ["DP-S", "DP-V", "DP-W"].map(str::to_string);
    let (result, staged_pids) =
        with_recorded_readiness(&st, &[old_static_pid, old_video_pid], || {
            reconcile_outputs(&st, &monitors, &ReconcileIntent::PolicyRefresh)
        });

    assert!(result.unwrap_err().to_string().contains("invalid WE id"));
    assert_eq!(staged_pids.len(), 2);
    for pid in staged_pids {
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }
    assert_eq!(
        st.renderers().take_output_still("DP-S").map(|renderer| {
            let pid = renderer.0.id();
            st.renderers().restore_output_still("DP-S", renderer);
            pid
        }),
        Some(old_static_pid)
    );
    assert_eq!(st.renderers().video_paper_pid("DP-V"), Some(old_video_pid));
    assert_eq!(st.renderers().assignments(), assignments_before);
    for pid in [old_static_pid, old_video_pid] {
        assert!(Path::new(&format!("/proc/{pid}")).exists());
    }
}

#[test]
fn apply_video_swaps_stdin() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    let _ready = st.readiness();
    apply_video(&st, "*", "/v/a.mp4", "fill", true, 80).unwrap();
    assert_eq!(wait_spawns(st.path(), 1), vec![vk_video_args("*", "/v/a.mp4", "fill", true, 80)]);
    assert!(st.renderers().has_video_paper("*"));
    apply_video(&st, "*", "/v/b.mp4", "fill", true, 80).unwrap();
    assert_eq!(settled_spawns(st.path()).len(), 1,);
    assert_eq!(recorded(&st)["*"], crate::audio::entry("video", "/v/b.mp4", "", true, 80));
    let mut children = st.renderers().take_all_video_papers();
    let stdin_file = st.path().join(format!("{}.stdin", children[0].0.id()));
    for (child, stdin) in &mut children {
        drop(stdin.take());
        let _ = child.wait();
    }
    assert_eq!(
        wait_stdin_lines(&stdin_file, 1),
        vec![serde_json::json!({"to": "/v/b.mp4", "mute": true, "volume": 80})]
    );
}

#[test]
fn vk_single_renderer() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    st.renderers().set_video_paper("GONE-1", sleeper(), None);
    let _ready = st.readiness();
    apply_video(&st, "*", "/v/a.mp4", "fill", true, 80).unwrap();
    assert_eq!(wait_spawns(st.path(), 1), vec![vk_video_args("*", "/v/a.mp4", "fill", true, 80)]);
    assert!(st.renderers().has_video_paper("*"));
    assert_eq!(recorded(&st)["*"], crate::audio::entry("video", "/v/a.mp4", "", true, 80));
    assert!(!st.renderers().has_video_paper("GONE-1"));
}

fn alive_at_ready_poller(
    st: &Stub,
    old_pid: u32,
) -> (Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let flag = Arc::new(AtomicBool::new(false));
    let state = st.state.clone();
    let alive = flag.clone();
    let handle = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let fresh: Vec<u32> = state
                .renderers()
                .wallpaper_pids()
                .into_iter()
                .filter(|pid| *pid != old_pid)
                .collect();
            if !fresh.is_empty() {
                std::thread::sleep(Duration::from_millis(50));
                alive.store(Path::new(&format!("/proc/{old_pid}")).exists(), Ordering::Relaxed);
                for pid in fresh {
                    state.renderers().signal_ready(pid);
                }
                return;
            }
            assert!(Instant::now() < deadline, "replacement renderer never registered");
            std::thread::sleep(Duration::from_millis(2));
        }
    });
    (flag, handle)
}

#[test]
fn still_kept_until_ready() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    let old = sleeper();
    let old_pid = old.id();
    st.renderers().set_output_still("DP-1", old, None);
    st.renderers().set_assignment("DP-1", "/w/old.png");
    seed(&st, "DP-1", "video", "/v/new.mp4", "", true, 50);
    let (alive_at_ready, handle) = alive_at_ready_poller(&st, old_pid);
    reconcile_outputs(&st, &["DP-1".to_string()], &ReconcileIntent::PolicyRefresh).unwrap();
    handle.join().unwrap();
    assert!(alive_at_ready.load(Ordering::Relaxed),);
    assert!(!Path::new(&format!("/proc/{old_pid}")).exists(),);
    assert!(st.renderers().has_video_paper("DP-1"));
    assert!(!st.renderers().has_output_still("DP-1"));
}

#[test]
fn timeout_keeps_surface() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    let old = sleeper();
    let old_pid = old.id();
    st.renderers().set_output_still("DP-1", old, None);
    st.renderers().set_assignment("DP-1", "/w/old.png");
    seed(&st, "DP-1", "video", "/v/new.mp4", "", true, 50);
    assert!(
        reconcile_outputs(&st, &["DP-1".to_string()], &ReconcileIntent::PolicyRefresh).is_err()
    );
    assert!(Path::new(&format!("/proc/{old_pid}")).exists(),);
    assert!(st.renderers().has_output_still("DP-1"));
    assert!(!st.renderers().has_video_paper("DP-1"));
}

#[test]
fn star_split_handoff() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    let star = sleeper();
    let star_pid = star.id();
    st.renderers().set_video_paper("*", star, None);
    seed(&st, "DP-1", "static", "/w/pin.png", "", true, 0);
    let (alive_at_ready, handle) = alive_at_ready_poller(&st, star_pid);
    reconcile_outputs(&st, &["DP-1".to_string()], &ReconcileIntent::PolicyRefresh).unwrap();
    handle.join().unwrap();
    assert!(alive_at_ready.load(Ordering::Relaxed),);
    assert!(!Path::new(&format!("/proc/{star_pid}")).exists(),);
    assert!(st.renderers().has_output_still("DP-1"));
}

#[test]
fn vk_swap_handoff() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    let old = sleeper();
    let old_pid = old.id();
    st.renderers().set_video_paper("*", old, None);
    let old_alive_at_ready = Arc::new(AtomicBool::new(false));
    let state = st.state.clone();
    let flag = old_alive_at_ready.clone();
    let handle = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let fresh: Vec<u32> = state
                .renderers()
                .wallpaper_pids()
                .into_iter()
                .filter(|pid| *pid != old_pid)
                .collect();
            if !fresh.is_empty() {
                std::thread::sleep(Duration::from_millis(50));
                flag.store(Path::new(&format!("/proc/{old_pid}")).exists(), Ordering::Relaxed);
                for pid in fresh {
                    state.renderers().signal_ready(pid);
                }
                return;
            }
            assert!(Instant::now() < deadline, "vk renderer never registered");
            std::thread::sleep(Duration::from_millis(2));
        }
    });
    apply_video(&st, "*", "/v/new.mp4", "fill", true, 80).unwrap();
    handle.join().unwrap();
    assert!(old_alive_at_ready.load(Ordering::Relaxed),);
    assert!(!Path::new(&format!("/proc/{old_pid}")).exists(),);
}

#[test]
fn apply_video_respawns_on_policy() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    let _ready = st.readiness();
    apply_video(&st, "*", "/v/a.mp4", "fill", true, 80).unwrap();
    let expected_policy = current_paper_policy(&st).signature();
    assert_eq!(st.renderers().policy(PAPER_POLICY_KEY).as_deref(), Some(expected_policy.as_str()));
    st.renderers().set_policy(PAPER_POLICY_KEY, "stale");
    apply_video(&st, "*", "/v/a.mp4", "fill", true, 80).unwrap();
    assert_eq!(wait_spawns(st.path(), 2).len(), 2);
}

#[test]
fn uniform_video_spawn_failure_restores() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    let old = sleeper();
    let old_pid = old.id();
    st.renderers().set_video_paper("*", old, None);
    st.renderers().set_assignment("DP-1", "/v/old.mp4");
    st.renderers().set_policy(PAPER_POLICY_KEY, "stale");
    std::fs::remove_file(st.config().renderer().vk_bin()).unwrap();

    let result = apply_video(&st, "*", "/v/new.mp4", "fill", true, 80);

    assert!(result.is_err());
    assert_eq!(st.renderers().video_paper_pid("*"), Some(old_pid));
    assert!(Path::new(&format!("/proc/{old_pid}")).exists());
    assert_eq!(st.renderers().assignments().get("DP-1").map(String::as_str), Some("/v/old.mp4"));
    assert_eq!(st.renderers().policy(PAPER_POLICY_KEY).as_deref(), Some("stale"));
}

#[test]
fn policy_refresh_keeps_uniform_state() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    let _ready = st.readiness();
    apply_video(&st, "*", "/v/a.mp4", "fill", true, 80).unwrap();
    let state_path = crate::audio::state_path(&st.config().cache_dir());
    let before = std::fs::read(&state_path).unwrap();
    st.renderers().set_policy(PAPER_POLICY_KEY, "stale");

    refresh_renderer_policy(&st).unwrap();

    assert_eq!(std::fs::read(&state_path).unwrap(), before);
    assert_eq!(settled_spawns(st.path()).len(), 2);
    assert!(paper_policy_matches(&st));
}

#[test]
fn policy_refresh_keeps_per_output() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    seed(&st, "DP-1", "video", "/v/a.mp4", "", true, 80);
    seed(&st, "DP-2", "static", "/w/b.png", "", true, 0);
    let monitors = vec!["DP-1".to_string(), "DP-2".to_string()];
    reconcile_ready(&st, "fill", &monitors, false, "", 0).unwrap();
    let state_path = crate::audio::state_path(&st.config().cache_dir());
    let before = std::fs::read(&state_path).unwrap();
    st.renderers().set_policy(PAPER_POLICY_KEY, "stale");

    refresh_policy_ready(&st).unwrap();

    assert_eq!(std::fs::read(&state_path).unwrap(), before);
    assert_eq!(st.renderers().assignments().get("DP-1").map(String::as_str), Some("/v/a.mp4"));
    assert_eq!(st.renderers().assignments().get("DP-2").map(String::as_str), Some("/w/b.png"));
    assert_eq!(settled_spawns(st.path()).len(), 3);
    assert!(paper_policy_matches(&st));
}

#[test]
fn policy_refresh_restarts_multi() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    seed(&st, "DP-1", "video", "/v/a.mp4", "", true, 80);
    seed(&st, "DP-2", "video", "/v/b.mp4", "", true, 80);
    let monitors = vec!["DP-1".to_string(), "DP-2".to_string()];
    reconcile_ready(&st, "fill", &monitors, false, "", 0).unwrap();
    let initial = settled_spawns(st.path());
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0].first().map(String::as_str), Some("--multi-json"));
    let manifest: Vec<paper_control::MultiVideoEntry> =
        serde_json::from_str(&initial[0][1]).expect("structured multi-video manifest");
    assert_eq!(
        manifest,
        vec![
            paper_control::MultiVideoEntry {
                output: "DP-1".to_string(),
                video: "/v/a.mp4".to_string(),
                mute: true,
                volume: 80,
                transition_from: None,
            },
            paper_control::MultiVideoEntry {
                output: "DP-2".to_string(),
                video: "/v/b.mp4".to_string(),
                mute: true,
                volume: 80,
                transition_from: None,
            },
        ]
    );

    st.renderers().set_policy(PAPER_POLICY_KEY, "stale");
    refresh_policy_ready(&st).unwrap();

    assert_eq!(settled_spawns(st.path()).len(), 2);
    assert!(paper_policy_matches(&st));
}

#[test]
fn multi_manifest_audio_and_from() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    let old_a = st.path().join("old-a.mp4");
    let old_b = st.path().join("old-b.mp4");
    let new_a = st.path().join("new-a.mp4");
    let new_b = st.path().join("new-b.mp4");
    for path in [&old_a, &old_b, &new_a, &new_b] {
        std::fs::write(path, b"fixture").unwrap();
    }
    st.renderers().set_assignment("DP-1", old_a.to_str().unwrap());
    st.renderers().set_assignment("DP-2", old_b.to_str().unwrap());
    seed(&st, "DP-1", "video", new_a.to_str().unwrap(), "", false, 31);
    seed(&st, "DP-2", "video", new_b.to_str().unwrap(), "", true, 76);

    reconcile_ready(
        &st,
        "fill",
        &["DP-1".to_string(), "DP-2".to_string()],
        true,
        "sand-donut",
        900,
    )
    .unwrap();

    let launches = settled_spawns(st.path());
    assert_eq!(launches.len(), 1);
    let args = &launches[0];
    assert_eq!(args.first().map(String::as_str), Some("--multi-json"));
    let manifest: Vec<paper_control::MultiVideoEntry> = serde_json::from_str(&args[1]).unwrap();
    assert_eq!((manifest[0].mute, manifest[0].volume), (false, 31));
    assert_eq!(manifest[0].transition_from.as_deref(), old_a.to_str());
    assert_eq!((manifest[1].mute, manifest[1].volume), (true, 76));
    assert_eq!(manifest[1].transition_from.as_deref(), old_b.to_str());
    assert!(args.windows(2).any(|pair| pair == ["--shader", "sand-donut"]));
    assert!(args.windows(2).any(|pair| pair == ["--duration-ms", "900"]));
}

#[test]
fn mixed_video_we_audio() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    let video_a = st.path().join("a.mp4");
    let video_b = st.path().join("b.mp4");
    std::fs::write(&video_a, b"fixture").unwrap();
    std::fs::write(&video_b, b"fixture").unwrap();
    let scene = st.path().join("we").join("100");
    std::fs::create_dir_all(&scene).unwrap();
    std::fs::write(scene.join("scene.pkg"), b"fixture").unwrap();
    seed(&st, "DP-1", "we", "", "100", false, 29);
    seed(&st, "DP-2", "video", video_a.to_str().unwrap(), "", true, 0);
    seed(&st, "DP-3", "video", video_b.to_str().unwrap(), "", false, 41);

    reconcile_ready(
        &st,
        "fill",
        &["DP-1".to_string(), "DP-2".to_string(), "DP-3".to_string()],
        true,
        "sand-globe",
        900,
    )
    .unwrap();

    let launches = settled_spawns(st.path());
    assert_eq!(launches.len(), 2);
    let multi = launches
        .iter()
        .find(|args| args.first().map(String::as_str) == Some("--multi-json"))
        .unwrap();
    let manifest: Vec<paper_control::MultiVideoEntry> = serde_json::from_str(&multi[1]).unwrap();
    assert_eq!(manifest.len(), 2);
    assert_eq!(
        (manifest[0].output.as_str(), manifest[0].mute, manifest[0].volume),
        ("DP-2", true, 0)
    );
    assert_eq!(
        (manifest[1].output.as_str(), manifest[1].mute, manifest[1].volume),
        ("DP-3", false, 41)
    );
    let scene = launches.iter().find(|args| args.iter().any(|arg| arg == "--scene")).unwrap();
    assert_eq!(scene.first().map(String::as_str), Some("DP-1"));
    assert!(scene.windows(2).any(|pair| pair == ["--mute", "false"]));
    assert!(scene.windows(2).any(|pair| pair == ["--volume", "29"]));
}

#[test]
fn duplicate_source_audio_resync() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    seed(&st, "DP-1", "video", "/v/same.mp4", "", false, 31);
    seed(&st, "DP-2", "video", "/v/same.mp4", "", false, 50);

    reconcile_ready(&st, "fill", &["DP-1".to_string(), "DP-2".to_string()], false, "", 0).unwrap();

    let mut children = st.renderers().take_all_video_papers();
    assert_eq!(children.len(), 1);
    let stdin_path = st.path().join(format!("{}.stdin", children[0].0.id()));
    for (child, stdin) in &mut children {
        drop(stdin.take());
        let _ = child.wait();
    }
    assert_eq!(
        wait_stdin_lines(&stdin_path, 2),
        vec![
            serde_json::json!({
                "to": "", "mute": true, "outputs": ["DP-2"]
            }),
            serde_json::json!({
                "to": "", "mute": false, "volume": 31,
                "outputs": ["DP-1", "DP-2"]
            }),
        ]
    );
    assert!(!recorded(&st)["DP-1"]["mute"].as_bool().unwrap());
    assert!(recorded(&st)["DP-2"]["mute"].as_bool().unwrap());
}

#[test]
fn per_output_spawn_failure_restores() {
    let st = Stub::new();
    let old = sleeper();
    let old_pid = old.id();
    st.renderers().set_video_paper("DP-1", old, None);
    st.renderers().set_assignment("DP-1", "/v/a.mp4");
    st.renderers().set_policy(PAPER_POLICY_KEY, "stale");
    seed(&st, "DP-1", "video", "/v/a.mp4", "", true, 80);
    std::fs::remove_file(st.config().renderer().vk_bin()).unwrap();

    let result = reconcile_outputs(&st, &["DP-1".to_string()], &ReconcileIntent::PolicyRefresh);

    assert!(result.is_err());
    assert_eq!(st.renderers().video_paper_pid("DP-1"), Some(old_pid));
    assert!(Path::new(&format!("/proc/{old_pid}")).exists());
    assert_eq!(st.renderers().assignments().get("DP-1").map(String::as_str), Some("/v/a.mp4"));
    assert!(!paper_policy_matches(&st));
}

#[test]
fn per_output_apply_failure_restores_exact_desired_state() {
    let st = Stub::new();
    seed(&st, "*", "video", "/v/old.mp4", "", true, 80);
    let previous = recorded(&st);
    std::fs::remove_file(st.config().renderer().vk_bin()).unwrap();

    let result = apply_output(&st, "DP-1", "video", "/v/new.mp4", "", "fill", true, 60);

    assert!(result.is_err());
    assert_eq!(recorded(&st), previous, "failed reconciliation must not stage outputs.json");
}

#[test]
fn reconcile_batch_failure_at_each_candidate_keeps_complete_renderer_set() {
    let _guard = crate::outputs::enum_shared();
    let outputs = ["DP-1", "DP-2", "DP-3"];
    for failed in 0..outputs.len() {
        let st = Stub::new();
        let mut incumbents = Vec::new();
        for (index, output) in outputs.iter().enumerate() {
            let child = sleeper();
            incumbents.push(((*output).to_string(), child.id()));
            st.renderers().set_output_still(output, child, None);
            st.renderers().set_assignment(output, &format!("/old/{index}.png"));
            if index == failed {
                seed(&st, output, "we", "", "missing-scene", true, 0);
            } else {
                seed(&st, output, "static", &format!("/new/{index}.png"), "", true, 0);
            }
        }

        let result = with_readiness(&st, || {
            reconcile_outputs(
                &st,
                &outputs.iter().map(ToString::to_string).collect::<Vec<_>>(),
                &ReconcileIntent::PolicyRefresh,
            )
        });

        assert!(result.is_err(), "candidate {failed} must abort the batch");
        for (index, (output, pid)) in incumbents.iter().enumerate() {
            let expected = format!("/old/{index}.png");
            assert!(st.renderers().output_still_pid_alive(output, *pid));
            assert_eq!(
                st.renderers().assignments().get(output).map(String::as_str),
                Some(expected.as_str())
            );
        }
    }
}

#[test]
fn vk_reuses_live() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    let out = st.path().join("vk-live.stdin");
    let (renderer, stdin) = capture_child(&out);
    let pid = renderer.id();
    {
        let mut child = renderer;
        let taken = child.stdin.take();
        let _ = taken;
        st.renderers().set_video_paper("*", child, stdin);
    }
    let _ready = st.readiness();
    apply_video(&st, "*", "/v/next.mp4", "fill", false, 65).unwrap();
    assert!(settled_spawns(st.path()).is_empty(),);
    assert_eq!(st.renderers().wallpaper_pids(), vec![pid]);
    apply_video_transition(&st, "/v/next.mp4", "/v/fade.mp4", "fill", "sand-bloom", 700, true, 80)
        .unwrap();
    assert!(settled_spawns(st.path()).is_empty());
    assert_eq!(
        wait_stdin_lines(&out, 2),
        vec![
            serde_json::json!({
                "to": "/v/next.mp4", "mute": false, "volume": 65
            }),
            serde_json::json!({
                "to": "/v/fade.mp4", "shader": "sand-bloom", "duration_ms": 700,
                "mute": true, "volume": 80
            }),
        ],
    );
    assert_eq!(recorded(&st)["*"], crate::audio::entry("video", "/v/fade.mp4", "", true, 80));
}

#[test]
fn vid_fade_reuses() {
    let st = Stub::new();
    let out = st.path().join("paper-live.stdin");
    let (child, stdin) = capture_child(&out);
    st.renderers().set_video_paper("*", child, stdin);
    let _ready = st.readiness();
    apply_video_transition(&st, "/v/old.mp4", "/v/new.mp4", "fill", "fade", 600, true, 90).unwrap();
    assert!(settled_spawns(st.path()).is_empty());
    assert!(st.renderers().has_video_paper("*"));
    assert_eq!(
        wait_stdin_lines(&out, 1),
        vec![serde_json::json!({
            "to": "/v/new.mp4", "shader": "fade", "duration_ms": 600,
            "mute": true, "volume": 90
        })]
    );
    assert_eq!(recorded(&st)["*"], crate::audio::entry("video", "/v/new.mp4", "", true, 90));
}

#[test]
fn vk_warm_swap_timeout_restores() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    let (renderer, stdin) = capture_child(&st.path().join("vk-timeout.stdin"));
    let renderer_pid = renderer.id();
    st.renderers().set_video_paper("*", renderer, stdin);
    let (still, still_stdin) = capture_child(&st.path().join("vk-still.stdin"));
    let still_pid = still.id();
    st.renderers().set_base_still(still, still_stdin);
    let holder = sleeper();
    let holder_pid = holder.id();
    st.renderers().track(holder);
    seed(&st, "*", "video", "/v/old.mp4", "", true, 50);
    record_paper_policy(&st);
    let policy = st.renderers().policy(PAPER_POLICY_KEY);

    let result =
        apply_video_transition(&st, "/v/old.mp4", "/v/new.mp4", "fill", "fade", 600, false, 75);

    assert!(result.is_err());
    assert_eq!(st.renderers().video_paper_pid("*"), Some(renderer_pid));
    assert!(Path::new(&format!("/proc/{renderer_pid}")).exists());
    assert!(st.renderers().has_base_still());
    assert!(Path::new(&format!("/proc/{still_pid}")).exists());
    assert_eq!(st.renderers().fleet_pids(), vec![holder_pid]);
    assert_eq!(recorded(&st)["*"], crate::audio::entry("video", "/v/old.mp4", "", true, 50));
    assert_eq!(st.renderers().policy(PAPER_POLICY_KEY), policy);
}

#[test]
fn overlay_timeout_restores() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    let (renderer, stdin) = capture_child(&st.path().join("overlay-timeout.stdin"));
    let renderer_pid = renderer.id();
    st.renderers().restore_paper((renderer, stdin));
    let covered = sleeper();
    let covered_pid = covered.id();
    st.renderers().set_video_paper("DP-1", covered, None);
    let (still, still_stdin) = capture_child(&st.path().join("overlay-still.stdin"));
    let still_pid = still.id();
    st.renderers().set_base_still(still, still_stdin);
    let holder = sleeper();
    let holder_pid = holder.id();
    st.renderers().track(holder);
    seed(&st, "*", "video", "/v/old.mp4", "", false, 60);
    record_paper_policy(&st);
    let policy = st.renderers().policy(PAPER_POLICY_KEY);

    let result = apply_video_transition(
        &st,
        "/v/old.mp4",
        "/v/new.mp4",
        "fill",
        "sand-bloom",
        700,
        true,
        90,
    );

    assert!(result.is_err());
    assert_eq!(st.renderers().paper_pid(), Some(renderer_pid));
    assert!(Path::new(&format!("/proc/{renderer_pid}")).exists());
    assert_eq!(st.renderers().video_paper_pid("DP-1"), Some(covered_pid));
    assert!(Path::new(&format!("/proc/{covered_pid}")).exists());
    assert!(st.renderers().has_base_still());
    assert!(Path::new(&format!("/proc/{still_pid}")).exists());
    assert_eq!(st.renderers().fleet_pids(), vec![holder_pid]);
    assert_eq!(recorded(&st)["*"], crate::audio::entry("video", "/v/old.mp4", "", false, 60));
    assert_eq!(st.renderers().policy(PAPER_POLICY_KEY), policy);
}

#[test]
fn img_fade_reaps_stills() {
    let st = Stub::new();
    let old = sleeper();
    let old_pid = old.id();
    st.renderers().set_base_still(old, None);
    let old_alive_at_ready = Arc::new(AtomicBool::new(false));
    let state = st.state.clone();
    let flag = old_alive_at_ready.clone();
    let handle = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(pid) =
                state.renderers().wallpaper_pids().into_iter().find(|pid| *pid != old_pid)
            {
                std::thread::sleep(Duration::from_millis(50));
                flag.store(Path::new(&format!("/proc/{old_pid}")).exists(), Ordering::Relaxed);
                state.renderers().signal_ready(pid);
                return;
            }
            assert!(Instant::now() < deadline, "video renderer never registered");
            std::thread::sleep(Duration::from_millis(2));
        }
    });
    apply_video_transition(&st, "/w/old.png", "/v/new.mp4", "fill", "fade", 600, false, 70)
        .unwrap();
    handle.join().unwrap();
    assert!(old_alive_at_ready.load(Ordering::Relaxed),);
    assert_eq!(
        wait_spawns(st.path(), 1),
        vec![video_transition_args(
            "*",
            "/w/old.png",
            "/v/new.mp4",
            "fill",
            "fade",
            600,
            false,
            70
        )]
    );
    assert!(!st.renderers().has_base_still());
    assert!(st.renderers().has_video_paper("*"));
    assert_eq!(recorded(&st)["*"], crate::audio::entry("video", "/v/new.mp4", "", false, 70));
}

#[test]
fn static_star_one_still() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    let _ready = st.readiness();
    apply_static_smart(&st, "*", "/w/a.png", "fill").unwrap();
    assert_eq!(
        wait_spawns(st.path(), 1),
        vec![vec!["*", "/w/a.png", "--fill-mode", "fill", "--persist"]]
    );
    assert_eq!(settled_spawns(st.path()).len(), 1,);
    assert!(st.renderers().has_base_still());
    assert_eq!(
        st.renderers().policy(PAPER_POLICY_KEY).as_deref(),
        Some(current_paper_policy(&st).signature().as_str())
    );
    let rec = recorded(&st);
    let map = rec.as_object().unwrap();
    assert!(!map.is_empty());
    for entry in map.values() {
        assert_eq!(*entry, crate::audio::entry("static", "/w/a.png", "", true, 100),);
    }
}

#[test]
fn static_reuses_base_still() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    let out = st.path().join("base-still.stdin");
    let (child, stdin) = capture_child(&out);
    st.renderers().set_base_still(child, stdin);
    st.renderers().set_video_paper("DP-1", sleeper(), None);
    st.renderers().set_output_still("DP-2", sleeper(), None);
    apply_static_smart(&st, "*", "/w/b.png", "fill").unwrap();
    assert!(settled_spawns(st.path()).is_empty());
    assert!(!st.renderers().has_video_paper("DP-1"));
    assert!(!st.renderers().has_output_still("DP-2"));
    let (mut child, stdin) = st.renderers().take_base_still().expect("base still stays registered");
    drop(stdin);
    let _ = child.wait();
    assert_eq!(
        wait_stdin_lines(&out, 1),
        vec![serde_json::json!({"path": "/w/b.png", "fill": "fill"})]
    );
}

#[test]
fn static_fade_overlay() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    let out = st.path().join("base-still.stdin");
    let (child, stdin) = capture_child(&out);
    st.renderers().set_base_still(child, stdin);
    st.renderers().set_output_still("DP-2", sleeper(), None);
    let _ready = st.readiness();
    apply_static_transition(&st, "/w/old.png", "/w/new.png", "fill", "fade", 600).unwrap();
    assert_eq!(
        wait_spawns(st.path(), 1),
        vec![managed_transition_args("/w/old.png", "/w/new.png", "fill", "fade", 600)]
    );
    assert!(st.renderers().has_base_still());
    let (mut overlay, stdin) =
        st.renderers().take_paper().expect("transition overlay remains owned until it exits");
    drop(stdin);
    let _ = overlay.wait();
    assert!(!st.renderers().has_output_still("DP-2"));
    let (mut child, stdin) = st.renderers().take_base_still().unwrap();
    drop(stdin);
    let _ = child.wait();
    assert_eq!(
        wait_stdin_lines(&out, 1),
        vec![serde_json::json!({"path": "/w/new.png", "fill": "fill"})]
    );
}

#[test]
fn session_keeps_transition_running() {
    let _guard = crate::outputs::enum_shared();
    let st = Stub::new();
    st.renderers().set_session_paused(11, true);
    st.renderers().begin_apply();
    let _ready = st.readiness();
    apply_static_transition(&st, "/w/old.png", "/w/new.png", "fill", "fade", 300).unwrap();
    let pid = st.renderers().paper_pid().unwrap();
    st.renderers().end_apply();
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(
        std::fs::read_to_string(st.path().join(format!("{pid}.stdin"))).unwrap_or_default(),
        ""
    );
    st.renderers().set_session_paused(11, false);
}
