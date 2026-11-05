use super::*;
use serde_json::json;

const MODELS: [&str; 8] = [
    "runtime.so",
    "lineart.onnx",
    "depth.onnx",
    "subject.onnx",
    "character.onnx",
    "salient.onnx",
    "faces.onnx",
    "heads.onnx",
];

fn pack_fixture() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    for model in MODELS {
        std::fs::write(directory.path().join(model), b"weights").unwrap();
    }
    let manifest = directory.path().join("processing.json");
    let value = json!({
        "version": 3,
        "runtime": MODELS[0],
        "device": "cpu",
        "lineart": MODELS[1],
        "depth": MODELS[2],
        "subject": MODELS[3],
        "character": MODELS[4],
        "salient": MODELS[5],
        "faces": MODELS[6],
        "heads": MODELS[7],
        "subject_classes": [0],
        "subject_confidence": 0.5
    });
    std::fs::write(&manifest, serde_json::to_vec(&value).unwrap()).unwrap();
    (directory, manifest)
}

fn save_pack(config: &skwd_wall_core::config::Config, pack: &pack::Pack) {
    let root = cache::root(config);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(cache::PACK_RECORD), serde_json::to_vec(pack).unwrap()).unwrap();
}

fn item(ctx: &Ctx, name: &str) -> (String, PathBuf) {
    let path = Path::new(&ctx.state.config().wallpaper_dir()).join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"original").unwrap();
    crate::testenv::cache_item(&ctx.state, name);
    (format!("{}{name}", wall_proto::drawing::STATIC_KEY_PREFIX), path)
}

fn toggle(ctx: &Ctx, key: &str, enabled: bool) -> Response {
    dispatch(
        ctx,
        &Request {
            id: 1,
            method: rpc::DRAWING_SET_ENABLED.into(),
            params: json!(SetEnabledRequest { key: key.into(), enabled }),
        },
    )
}

const PREPARES: &str = r#"#!/usr/bin/env python3
import json
import pathlib
import sys
import time

args = sys.argv[1:]
if "--check" in args:
    sys.exit(0)
time.sleep(0.3)
source = pathlib.Path(args[args.index("--input") + 1])
output = pathlib.Path(args[args.index("--output") + 1])
sha = args[args.index("--source-sha256") + 1]
output.mkdir()
(output / "ink.png").write_bytes(b"ink")
(output / "strokes.bin").write_bytes(b"strokes")
stat = source.stat()
manifest = {"version": 3, "source": {"path": str(source), "size": stat.st_size,
    "mtime_ns": stat.st_mtime_ns, "sha256": sha}, "width": 10, "height": 10,
    "texture": "ink.png", "strokes": "strokes.bin", "timing_width": 8, "timing_height": 8, "focus": [0.5, 0.5]}
(output / "manifest.json").write_text(json.dumps(manifest))
"#;
const FAILS: &str = "#!/bin/sh\nexit 1\n";

struct Worker {
    bin: crate::testenv::FakeBin,
    pack: tempfile::TempDir,
}

fn worker_pack(root: &Path, script: &str) -> (Worker, PathBuf) {
    let bin = crate::testenv::FakeBin::install(root, skwd_wall_core::paths::DRAW_BIN, script);
    let (pack, manifest) = pack_fixture();
    (Worker { bin, pack }, manifest)
}

#[test]
fn python_pack_invalid() {
    let (_guard, _) = crate::testenv::lock();
    let directory = tempfile::tempdir().unwrap();
    crate::testenv::write_config(json!({"paths": {"cache": directory.path()}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let manifest = directory.path().join("processing.json");
    std::fs::write(&manifest, br#"{"version":1,"python":"/usr/bin/python3","device":"cuda"}"#)
        .unwrap();
    let legacy = pack::Pack {
        format: pack::RECORD_FORMAT,
        path: manifest.clone(),
        fingerprint: cache::digest(b"legacy"),
        bytes: 0,
        inventory: vec![cache::PackStamp::capture(&manifest).unwrap()],
        device: wall_proto::drawing::Device::Auto,
    };
    save_pack(&ctx.state.config(), &legacy);
    let error = pack::Pack::load(&ctx.state.config()).unwrap_err();
    assert!(error.to_string().contains("unsupported drawing pack version"));
    assert_eq!(ctx.drawing.status(&ctx, "").pack, PackState::Invalid);
}

#[test]
fn rejects_older_record() {
    let (_guard, _) = crate::testenv::lock();
    let (directory, manifest) = pack_fixture();
    crate::testenv::write_config(json!({"paths": {"cache": directory.path()}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let mut record = serde_json::to_value(pack::Pack::inspect(&manifest).unwrap()).unwrap();
    record.as_object_mut().unwrap().remove("format");
    let root = cache::root(&ctx.state.config());
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(cache::PACK_RECORD), record.to_string()).unwrap();
    assert!(pack::Pack::load(&ctx.state.config()).is_err());
    assert_eq!(ctx.drawing.status(&ctx, "").pack, PackState::Invalid);
}

#[test]
fn fingerprint_skips_recipe() {
    let (_directory, manifest) = pack_fixture();
    let mut expected = std::fs::read(&manifest).unwrap();
    for file in cache::pack::Pack::read(&manifest).unwrap().files() {
        let file = file.canonicalize().unwrap();
        expected.extend_from_slice(file.as_os_str().as_encoded_bytes());
        expected.extend_from_slice(cache::hash_file(&file).unwrap().as_bytes());
    }
    assert_eq!(pack::Pack::inspect(&manifest).unwrap().fingerprint, cache::digest(&expected));
}

#[test]
fn starts_without_models() {
    let (_guard, _) = crate::testenv::lock();
    crate::testenv::write_config(json!({}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let status = ctx.drawing.status(&ctx, "");
    assert_eq!(status.pack, PackState::Missing);
    assert!(!status.prepared);
}

#[test]
fn rejects_missing_media() {
    let (_guard, _) = crate::testenv::lock();
    crate::testenv::write_config(json!({}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    assert!(source(&ctx, "video:missing-drawing-video.mp4").is_err());
    assert!(source(&ctx, "we:missing-drawing-item").is_err());
}

#[test]
fn accepts_video_key() {
    let (_guard, _) = crate::testenv::lock();
    crate::testenv::write_config(json!({}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let path = Path::new(&ctx.state.config().video_dir()).join("clip.mp4");
    std::fs::write(&path, b"video").unwrap();
    ctx.state
        .with_db(|connection| {
            skwd_wall_core::db::upsert_cache_entry(
                connection,
                "video:clip.mp4",
                "video",
                "clip.mp4",
                "",
                "",
                &path.to_string_lossy(),
                "",
                1,
                0,
                0,
                0,
                5,
                1,
                1,
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(source(&ctx, "video:clip.mp4").unwrap(), path);
    assert!(source(&ctx, "video:../outside.mp4").is_err());
}

#[test]
fn accepts_we_media() {
    let (_guard, root) = crate::testenv::lock();
    let workshop = root.join("workshop");
    let item = workshop.join("42");
    std::fs::create_dir_all(&item).unwrap();
    std::fs::write(item.join("clip.mp4"), b"video").unwrap();
    std::fs::write(item.join("project.json"), br#"{"type":"video","file":"clip.mp4"}"#).unwrap();
    crate::testenv::write_config(json!({"paths":{"steamWorkshop":workshop}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    ctx.state
        .with_db(|connection| {
            skwd_wall_core::db::upsert_cache_entry(
                connection, "we:42", "we", "Movie", "", "", "", "42", 1, 0, 0, 0, 5, 1, 1,
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(source(&ctx, "we:42").unwrap(), item);
    std::fs::write(item.join("project.json"), br#"{"type":"scene"}"#).unwrap();
    std::fs::write(item.join("scene.pkg"), b"scene").unwrap();
    assert_eq!(source(&ctx, "we:42").unwrap(), item);
    std::fs::write(item.join("project.json"), br#"{"type":"web","file":"index.html"}"#).unwrap();
    assert!(source(&ctx, "we:42").is_err());
}

#[test]
fn rejects_unknown_pack() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("processing.json");
    std::fs::write(&path, br#"{"version":88}"#).unwrap();
    assert!(pack::Pack::inspect(&path).is_err());
}

#[test]
fn pause_leaves_cache() {
    let (_guard, _) = crate::testenv::lock();
    crate::testenv::write_config(json!({}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let root = cache::artifacts(&ctx.state.config());
    std::fs::create_dir_all(&root).unwrap();
    let prepared = root.join("fixture");
    std::fs::write(&prepared, b"prepared").unwrap();
    ctx.drawing.cancel(&ctx, Phase::Paused);
    assert!(prepared.exists());
    assert_eq!(ctx.drawing.status(&ctx, "").phase, Phase::Paused);
}

#[test]
fn rejects_oversized_queue() {
    let (_guard, _) = crate::testenv::lock();
    crate::testenv::write_config(json!({}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let keys = vec![String::from("static:one.png"); MAX_PREPARE_KEYS + 1];
    assert!(ctx.drawing.prepare(&ctx, keys, BTreeSet::new(), BTreeSet::new()).is_err());
    assert!(skwd_wall_core::lock(&ctx.drawing.job).thread.is_none());
}

#[test]
fn separates_selected_status() {
    let (_guard, _) = crate::testenv::lock();
    crate::testenv::write_config(json!({}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        job.status.active_key = "static:first.png".into();
        job.status.phase = Phase::Failed;
        job.status.error = Some("first failed".into());
    }
    let status = ctx.drawing.status(&ctx, "static:second.png");
    assert_eq!(status.phase, Phase::Idle);
    assert!(status.error.is_none());
    assert_eq!(status.active_key, "static:first.png");
}

#[test]
fn detects_changed_model() {
    use std::os::unix::fs::MetadataExt;
    use std::time::{Duration, Instant};

    let (directory, manifest) = pack_fixture();
    let config = skwd_wall_core::config::Config::from_root(json!({
        "paths": {"cache": directory.path()}
    }));
    let pack = pack::Pack::inspect(&manifest).unwrap();
    save_pack(&config, &pack);
    let model = directory.path().join(MODELS[1]);
    let initial = model.metadata().unwrap();
    let modified = initial.modified().unwrap();
    let file = std::fs::File::options().write(true).open(&model).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        std::fs::write(&model, b"changed").unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(modified)).unwrap();
        let current = file.metadata().unwrap();
        if (current.ctime(), current.ctime_nsec()) != (initial.ctime(), initial.ctime_nsec()) {
            break;
        }
        assert!(Instant::now() < deadline, "ctime unchanged");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(model.metadata().unwrap().ino(), initial.ino());
    assert_eq!(model.metadata().unwrap().len(), 7);
    assert_eq!(model.metadata().unwrap().modified().unwrap(), modified);
    assert!(pack::Pack::load(&config).is_err());
    assert!(cache::validate_processing(&config).is_err());
}

#[test]
fn inventory_covers_extras() {
    let (directory, manifest) = pack_fixture();
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    std::fs::write(directory.path().join("webgpu.so"), b"weights").unwrap();
    value["webgpu"] = json!("webgpu.so");
    std::fs::write(&manifest, serde_json::to_vec(&value).unwrap()).unwrap();
    let pack = pack::Pack::inspect(&manifest).unwrap();
    assert_eq!(pack.inventory.len(), MODELS.len() + 2);
    std::fs::remove_file(directory.path().join("webgpu.so")).unwrap();
    assert!(pack::Pack::inspect(&manifest).is_err());
}

#[test]
fn cancels_pack_scan() {
    let (_directory, manifest) = pack_fixture();
    let cancel = AtomicBool::new(true);
    assert!(pack::Pack::inspect_cancellable(&manifest, &cancel).is_err());
}

#[test]
fn shows_failed_candidate() {
    let (_guard, _) = crate::testenv::lock();
    let (directory, manifest) = pack_fixture();
    crate::testenv::write_config(json!({"paths": {"cache": directory.path()}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let config = ctx.state.config();
    let pack = pack::Pack::inspect(&manifest).unwrap();
    save_pack(&config, &pack);
    {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        job.status.phase = Phase::Failed;
        job.status.pack_path = directory.path().join("replacement.json").display().to_string();
        job.status.error = Some("invalid replacement".into());
    }
    let status = ctx.drawing.status(&ctx, "");
    assert_eq!(status.pack, PackState::Invalid);
    assert_eq!(status.error.as_deref(), Some("invalid replacement"));
    assert!(status.pack_path.ends_with("replacement.json"));
}

#[test]
fn keeps_checking_candidate() {
    let (_guard, _) = crate::testenv::lock();
    let (directory, manifest) = pack_fixture();
    crate::testenv::write_config(json!({"paths": {"cache": directory.path()}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let config = ctx.state.config();
    let pack = pack::Pack::inspect(&manifest).unwrap();
    save_pack(&config, &pack);
    let candidate = directory.path().join("candidate.json").display().to_string();
    {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        job.status.phase = Phase::Checking;
        job.status.pack_path.clone_from(&candidate);
    }
    let status = ctx.drawing.status(&ctx, "");
    assert_eq!(status.phase, Phase::Checking);
    assert_eq!(status.pack_path, candidate);
}

#[test]
fn checked_pack_prepares() {
    let (_guard, root) = crate::testenv::lock();
    let _bin = crate::testenv::FakeBin::install(&root, skwd_wall_core::paths::DRAW_BIN, PREPARES);
    let (directory, manifest) = pack_fixture();
    crate::testenv::write_config(json!({"paths": {"cache": directory.path()}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let (release, held) = std::sync::mpsc::channel::<()>();
    {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        job.status.phase = Phase::Checking;
        job.thread = Some(std::thread::spawn(move || held.recv().unwrap()));
    }
    run::validate(&ctx, &manifest, &AtomicBool::new(false));
    assert_eq!(ctx.drawing.status(&ctx, "").pack, PackState::Available);
    ctx.drawing.prepare_library(&ctx, false).unwrap();
    release.send(()).unwrap();
    let thread = skwd_wall_core::lock(&ctx.drawing.job).thread.take().unwrap();
    thread.join().unwrap();
}

fn system_pack() -> tempfile::TempDir {
    let (directory, manifest) = pack_fixture();
    std::fs::rename(&manifest, directory.path().join(pack::SYSTEM_MANIFEST)).unwrap();
    unsafe { std::env::set_var(pack::SYSTEM_HOME_ENV, directory.path()) };
    directory
}

#[test]
fn adopts_system_pack() {
    let (_guard, root) = crate::testenv::lock();
    let _bin = crate::testenv::FakeBin::install(&root, skwd_wall_core::paths::DRAW_BIN, PREPARES);
    let cache_root = tempfile::tempdir().unwrap();
    crate::testenv::write_config(json!({"paths": {"cache": cache_root.path()}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let _system = system_pack();
    assert_eq!(ctx.drawing.status(&ctx, "").phase, Phase::Checking);
    let thread = skwd_wall_core::lock(&ctx.drawing.job).thread.take().unwrap();
    thread.join().unwrap();
    assert_eq!(ctx.drawing.status(&ctx, "").pack, PackState::Available);
}

#[test]
fn rechecks_system_pack() {
    let (_guard, root) = crate::testenv::lock();
    let _bin = crate::testenv::FakeBin::install(&root, skwd_wall_core::paths::DRAW_BIN, PREPARES);
    let cache_root = tempfile::tempdir().unwrap();
    let system = system_pack();
    let manifest = system.path().join(pack::SYSTEM_MANIFEST);
    crate::testenv::write_config(
        json!({"paths": {"cache": cache_root.path()}, "drawing": {"pack": manifest}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    assert_eq!(ctx.drawing.status(&ctx, "").phase, Phase::Checking);
    let thread = skwd_wall_core::lock(&ctx.drawing.job).thread.take().unwrap();
    thread.join().unwrap();
}

#[test]
fn registered_pack_decides_once() {
    let (_guard, _) = crate::testenv::lock();
    let (directory, manifest) = pack_fixture();
    crate::testenv::write_config(json!({"paths": {"cache": directory.path()}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let _system = system_pack();
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());
    let status = ctx.drawing.status(&ctx, "");
    assert_eq!((status.phase, status.pack), (Phase::Idle, PackState::Available));
    assert!(skwd_wall_core::lock(&ctx.drawing.job).system_checked);
    std::fs::remove_file(cache::root(&ctx.state.config()).join(cache::PACK_RECORD)).unwrap();
    let status = ctx.drawing.status(&ctx, "");
    assert_eq!((status.phase, status.pack), (Phase::Idle, PackState::Missing));
}

#[test]
fn chosen_pack_wins() {
    let (_guard, _) = crate::testenv::lock();
    let cache_root = tempfile::tempdir().unwrap();
    let _system = system_pack();
    for chosen in ["", "/nonexistent/pack.json"] {
        crate::testenv::write_config(
            json!({"paths": {"cache": cache_root.path()}, "drawing": {"pack": chosen}}),
        );
        let (state, events, stats) = crate::testenv::harness();
        let ctx = crate::testenv::context(&state, &events, &stats);
        let status = ctx.drawing.status(&ctx, "");
        assert_eq!((status.phase, status.pack), (Phase::Idle, PackState::Missing), "{chosen}");
    }
}

#[test]
fn detects_retargeted_model() {
    let (directory, manifest) = pack_fixture();
    let original = directory.path().join(MODELS[1]);
    let replacement = directory.path().join("replacement.bin");
    let link = directory.path().join("current.bin");
    std::fs::write(&replacement, b"updated").unwrap();
    std::os::unix::fs::symlink(&original, &link).unwrap();
    let mut value: serde_json::Value = cache::read_record(&manifest).unwrap();
    value["lineart"] = json!(link);
    std::fs::write(&manifest, serde_json::to_vec(&value).unwrap()).unwrap();
    let config = skwd_wall_core::config::Config::from_root(json!({
        "paths":{"cache":directory.path()}
    }));
    let pack = pack::Pack::inspect(&manifest).unwrap();
    save_pack(&config, &pack);
    assert!(pack::Pack::load(&config).is_ok());
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(&replacement, &link).unwrap();
    assert!(pack::Pack::load(&config).is_err());
    assert!(cache::validate_processing(&config).is_err());
}

#[test]
fn cached_card_isolated() {
    let (_guard, _) = crate::testenv::lock();
    let cache_root = tempfile::tempdir().unwrap();
    crate::testenv::write_config(
        json!({"paths": {"cache": cache_root.path()}, "drawing": {"enabled": false}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let (first, source) = item(&ctx, "first.webp");
    let (second, _) = item(&ctx, "first-extra.webp");
    let artifact = crate::testenv::drawn(&ctx.state.config(), &source);
    {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        job.status.active_key.clone_from(&first);
        job.status.phase = Phase::Failed;
        job.status.error = Some("earlier preparation failed".into());
    }

    let enabled = toggle(&ctx, &first, true);
    assert!(enabled.error.is_none());
    assert_eq!(enabled.result.unwrap()["phase"], "ready");
    assert!(ctx.drawing.status(&ctx, &first).enabled);
    assert!(!ctx.drawing.status(&ctx, &second).enabled);
    assert!(skwd_config::drawing_enabled(skwd_wall_core::config::Config::load().root(), &first));

    assert!(toggle(&ctx, &first, false).error.is_none());
    assert!(!ctx.drawing.status(&ctx, &first).enabled);
    assert!(artifact.exists());
    assert!(ctx.drawing.status(&ctx, &first).prepared);
}

#[test]
fn outline_prepares_still() {
    let (_guard, root) = crate::testenv::lock();
    let cache_root = tempfile::tempdir().unwrap();
    crate::testenv::write_config(json!({"paths": {"cache": cache_root.path()}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let (key, source) = item(&ctx, "shown.png");
    let (_directory, manifest) = worker_pack(&root, PREPARES);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());

    outline(&ctx, &source);
    let thread = skwd_wall_core::lock(&ctx.drawing.job).thread.take().unwrap();
    thread.join().unwrap();
    assert!(cache::lookup(&ctx.state.config(), &source).is_ok());
    assert!(!skwd_config::drawing_enabled(ctx.state.config().root(), &key));

    outline(&ctx, &source);
    assert!(skwd_wall_core::lock(&ctx.drawing.job).thread.is_none());
}

struct Shown {
    ctx: Ctx,
    key: String,
    source: PathBuf,
    _cache: tempfile::TempDir,
    _pack: tempfile::TempDir,
}

fn shown() -> Shown {
    let cache = tempfile::tempdir().unwrap();
    crate::testenv::write_config(json!({"paths": {"cache": cache.path()}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let (key, source) = item(&ctx, "shown.png");
    let (pack, manifest) = pack_fixture();
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());
    Shown { ctx, key, source, _cache: cache, _pack: pack }
}

fn busy(ctx: &Ctx, library: bool, queued: &str) {
    let mut job = skwd_wall_core::lock(&ctx.drawing.job);
    job.activity = Activity::Busy;
    job.library = library;
    job.status.active_key = "static:running.png".into();
    job.pending.push_back(queued.into());
}

#[test]
fn outline_follows_card() {
    let (_guard, _) = crate::testenv::lock();
    let Shown { ctx, key, source, .. } = &shown();
    busy(ctx, false, "static:card.png");
    outline(ctx, source);
    outline(ctx, source);
    assert_eq!(skwd_wall_core::lock(&ctx.drawing.job).pending, ["static:card.png", key.as_str()]);
}

#[test]
fn outline_leads_library() {
    let (_guard, _) = crate::testenv::lock();
    let Shown { ctx, key, source, .. } = &shown();
    busy(ctx, true, "static:later.png");
    outline(ctx, source);
    assert_eq!(skwd_wall_core::lock(&ctx.drawing.job).pending, [key.as_str(), "static:later.png"]);
}

#[test]
fn outline_skips_running() {
    let (_guard, _) = crate::testenv::lock();
    let Shown { ctx, key, source, .. } = &shown();
    busy(ctx, false, "static:card.png");
    skwd_wall_core::lock(&ctx.drawing.job).status.active_key.clone_from(key);
    outline(ctx, source);
    assert_eq!(skwd_wall_core::lock(&ctx.drawing.job).pending, ["static:card.png"]);
}

#[test]
fn outline_respects_hold() {
    let (_guard, _) = crate::testenv::lock();
    let Shown { ctx, source, .. } = &shown();
    batch::hold(&ctx.state.config(), true).unwrap();
    outline(ctx, source);
    assert!(skwd_wall_core::lock(&ctx.drawing.job).thread.is_none());
}

#[test]
fn outline_respects_pause() {
    let (_guard, _) = crate::testenv::lock();
    let Shown { ctx, source, .. } = &shown();
    skwd_wall_core::lock(&ctx.drawing.job).status.phase = Phase::Paused;
    outline(ctx, source);
    assert!(skwd_wall_core::lock(&ctx.drawing.job).thread.is_none());
}

#[test]
fn outline_keeps_failed_library() {
    let (_guard, _) = crate::testenv::lock();
    let Shown { ctx, source, .. } = &shown();
    {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        job.library = true;
        job.status.phase = Phase::Failed;
        job.pending.push_back("static:later.png".into());
    }
    outline(ctx, source);
    let job = skwd_wall_core::lock(&ctx.drawing.job);
    assert!(job.library && job.thread.is_none());
    assert_eq!(job.pending, ["static:later.png"]);
}

#[test]
fn outline_keeps_failed_card() {
    let (_guard, _) = crate::testenv::lock();
    let Shown { ctx, source, .. } = &shown();
    {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        job.status.phase = Phase::Failed;
        job.pending.extend(["static:card.png".into(), "static:next.png".into()]);
    }
    outline(ctx, source);
    let job = skwd_wall_core::lock(&ctx.drawing.job);
    assert!(job.thread.is_none());
    assert_eq!(job.pending, ["static:card.png", "static:next.png"]);
}

#[test]
fn outline_replaces_failed_card() {
    let (_guard, root) = crate::testenv::lock();
    let Shown { ctx, source, .. } = &shown();
    let (_directory, manifest) = worker_pack(&root, PREPARES);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());
    {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        job.status.phase = Phase::Failed;
        job.pending.push_back("static:card.png".into());
        job.enable.insert("static:card.png".into());
    }
    outline(ctx, source);
    let thread = skwd_wall_core::lock(&ctx.drawing.job).thread.take().unwrap();
    thread.join().unwrap();
    assert!(cache::lookup(&ctx.state.config(), source).is_ok());
    assert!(skwd_wall_core::lock(&ctx.drawing.job).enable.is_empty());
}

#[test]
fn outline_leaves_own_failure() {
    let (_guard, _) = crate::testenv::lock();
    let Shown { ctx, key, source, .. } = &shown();
    {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        job.status.phase = Phase::Failed;
        job.status.active_key.clone_from(key);
        job.pending.push_back(key.clone());
    }
    outline(ctx, source);
    let job = skwd_wall_core::lock(&ctx.drawing.job);
    assert!(job.thread.is_none() && job.outlines.is_empty());
    assert_eq!(job.status.phase, Phase::Failed);
}

#[test]
fn outline_leaves_explicit() {
    let (_guard, _) = crate::testenv::lock();
    let Shown { ctx, key, source, .. } = &shown();
    busy(ctx, false, key);
    outline(ctx, source);
    let job = skwd_wall_core::lock(&ctx.drawing.job);
    assert!(job.outlines.is_empty());
    assert_eq!(job.pending, [key.as_str()]);
}

#[test]
fn outline_failure_skips() {
    let (_guard, root) = crate::testenv::lock();
    let Shown { ctx, source, .. } = &shown();
    let (_directory, manifest) = worker_pack(&root, FAILS);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());
    outline(ctx, source);
    let thread = skwd_wall_core::lock(&ctx.drawing.job).thread.take().unwrap();
    thread.join().unwrap();
    let job = skwd_wall_core::lock(&ctx.drawing.job);
    assert_eq!(job.status.phase, Phase::Ready);
    assert!(job.failed.is_empty() && job.pending.is_empty() && job.outlines.is_empty());
}

#[test]
fn prepares_card_then_enables() {
    let (_guard, root) = crate::testenv::lock();
    let cache_root = tempfile::tempdir().unwrap();
    crate::testenv::write_config(
        json!({"paths": {"cache": cache_root.path()}, "drawing": {"enabled": false}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let mut receiver = crate::testenv::subscribe(&events);
    let (key, source) = item(&ctx, "prepared.png");
    let (_directory, manifest) = worker_pack(&root, PREPARES);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());

    let response = toggle(&ctx, &key, true);
    assert!(response.error.is_none(), "{:?}", response.error);
    assert!(!response.result.unwrap()["enabled"].as_bool().unwrap());
    let thread = skwd_wall_core::lock(&ctx.drawing.job).thread.take().unwrap();
    thread.join().unwrap();
    let status = ctx.drawing.status(&ctx, &key);
    assert_eq!(status.phase, Phase::Ready);
    assert!(status.enabled);
    assert!(status.prepared);
    assert!(cache::lookup(&ctx.state.config(), &source).is_ok());
    assert!(skwd_config::drawing_enabled(skwd_wall_core::config::Config::load().root(), &key));
    let published = crate::testenv::events(&mut receiver);
    let config_at = published.iter().position(|event| event.event == ev::CONFIG_CHANGED).unwrap();
    let ready_at = published
        .iter()
        .position(|event| event.event == ev::DRAWING_STATUS && event.data["phase"] == "ready")
        .unwrap();
    assert!(config_at < ready_at);
}

#[test]
fn prepares_video_reference() {
    use std::os::unix::fs::PermissionsExt;

    let (_guard, root) = crate::testenv::lock();
    let cache_root = tempfile::tempdir().unwrap();
    let (worker, manifest) = worker_pack(&root, PREPARES);
    let paper = worker.pack.path().join("paper.py");
    let poster = worker.pack.path().join("reference.webp");
    image::RgbImage::new(2, 2).save(&poster).unwrap();
    let script = r#"#!/usr/bin/env python3
import json
import pathlib
import sys

request = json.loads(sys.stdin.readline())
assert request["reference"] is True
root = pathlib.Path(__file__).parent
pathlib.Path(request["destination"]).write_bytes((root / "reference.webp").read_bytes())
with (root / "captures.txt").open("a") as log:
    log.write("capture\n")
print(json.dumps({"source": request["source"], "error": None}))
"#;
    std::fs::write(&paper, script).unwrap();
    std::fs::set_permissions(&paper, std::fs::Permissions::from_mode(0o700)).unwrap();
    crate::testenv::write_config(json!({
        "paths":{"cache":cache_root.path(),"paperBin":paper},
        "drawing":{"enabled":false}
    }));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let path = Path::new(&ctx.state.config().video_dir()).join("prepared.mp4");
    std::fs::write(&path, b"video").unwrap();
    let key = "video:prepared.mp4";
    ctx.state
        .with_db(|connection| {
            skwd_wall_core::db::upsert_cache_entry(
                connection,
                key,
                "video",
                "prepared.mp4",
                "",
                "",
                &path.to_string_lossy(),
                "",
                1,
                0,
                0,
                0,
                5,
                1,
                1,
            )?;
            Ok(())
        })
        .unwrap();
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());
    assert!(toggle(&ctx, key, true).error.is_none());
    let thread = skwd_wall_core::lock(&ctx.drawing.job).thread.take().unwrap();
    thread.join().unwrap();
    let ready = ctx.drawing.status(&ctx, key);
    assert_eq!(ready.phase, Phase::Ready, "{:?}", ready.error);
    assert!(ready.prepared);
    assert!(ready.enabled);
    let signature = cache::media_signature(&ctx.state, key, &path).unwrap();
    let reference = cache::lookup_media(&ctx.state.config(), &path, &signature).unwrap().1;
    std::fs::write(&reference, b"broken poster").unwrap();
    assert!(!ctx.drawing.status(&ctx, key).prepared);
    assert!(toggle(&ctx, key, true).error.is_none());
    let thread = skwd_wall_core::lock(&ctx.drawing.job).thread.take().unwrap();
    thread.join().unwrap();
    assert!(ctx.drawing.status(&ctx, key).prepared);
    assert!(image::open(&reference).is_ok());
    assert_eq!(
        std::fs::read_to_string(worker.pack.path().join("captures.txt")).unwrap().lines().count(),
        2
    );
    std::fs::write(&path, b"changed video").unwrap();
    assert!(!ctx.drawing.status(&ctx, key).prepared);
}

#[test]
fn failed_card_stays_off() {
    let (_guard, root) = crate::testenv::lock();
    let cache_root = tempfile::tempdir().unwrap();
    crate::testenv::write_config(
        json!({"paths": {"cache": cache_root.path()}, "drawing": {"enabled": false}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let (key, _) = item(&ctx, "failed.png");
    let (_directory, manifest) = worker_pack(&root, FAILS);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());

    assert!(toggle(&ctx, &key, true).error.is_none());
    let thread = skwd_wall_core::lock(&ctx.drawing.job).thread.take().unwrap();
    thread.join().unwrap();
    let status = ctx.drawing.status(&ctx, &key);
    assert_eq!(status.phase, Phase::Failed);
    assert!(!status.enabled);
    assert!(!status.prepared);
    assert!(!skwd_config::drawing_enabled(skwd_wall_core::config::Config::load().root(), &key));
}

#[test]
fn busy_card_stays_off() {
    let (_guard, root) = crate::testenv::lock();
    let cache_root = tempfile::tempdir().unwrap();
    crate::testenv::write_config(
        json!({"paths": {"cache": cache_root.path()}, "drawing": {"enabled": false}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let (key, _) = item(&ctx, "busy.png");
    let (other, _) = item(&ctx, "other.png");
    let (_directory, manifest) = worker_pack(&root, PREPARES);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());
    {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        job.activity = Activity::Waiting;
        job.thread =
            Some(std::thread::spawn(|| std::thread::sleep(std::time::Duration::from_secs(1))));
        job.enable.insert(other);
    }
    assert!(toggle(&ctx, &key, true).error.is_some());
    assert!(!ctx.drawing.status(&ctx, &key).enabled);
    assert!(toggle(&ctx, &key, false).error.is_none());
    assert!(skwd_wall_core::lock(&ctx.drawing.job).thread.is_some());
    ctx.drawing.cancel(&ctx, Phase::Cancelled);
}

#[test]
fn missing_pack_stays_off() {
    let (_guard, _) = crate::testenv::lock();
    let cache_root = tempfile::tempdir().unwrap();
    crate::testenv::write_config(
        json!({"paths": {"cache": cache_root.path()}, "drawing": {"enabled": false}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let (key, _) = item(&ctx, "missing.png");
    let response = toggle(&ctx, &key, true);
    assert!(response.error.is_some());
    assert!(!ctx.drawing.status(&ctx, &key).enabled);
}

#[test]
fn cancelled_card_stays_off() {
    let (_guard, root) = crate::testenv::lock();
    let cache_root = tempfile::tempdir().unwrap();
    crate::testenv::write_config(
        json!({"paths": {"cache": cache_root.path()}, "drawing": {"enabled": false}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let (key, _) = item(&ctx, "cancelled.png");
    let (_directory, manifest) = worker_pack(&root, PREPARES);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());

    assert!(toggle(&ctx, &key, true).error.is_none());
    ctx.drawing.cancel(&ctx, Phase::Cancelled);
    assert_eq!(ctx.drawing.status(&ctx, &key).phase, Phase::Cancelled);
    assert!(!ctx.drawing.status(&ctx, &key).enabled);
    assert!(!skwd_config::drawing_enabled(skwd_wall_core::config::Config::load().root(), &key));
}

#[test]
fn paused_card_resumes() {
    let (_guard, root) = crate::testenv::lock();
    let cache_root = tempfile::tempdir().unwrap();
    crate::testenv::write_config(
        json!({"paths": {"cache": cache_root.path()}, "drawing": {"enabled": false}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let (key, _) = item(&ctx, "paused.png");
    let (_directory, manifest) = worker_pack(&root, PREPARES);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());

    assert!(toggle(&ctx, &key, true).error.is_none());
    ctx.drawing.cancel(&ctx, Phase::Paused);
    assert_eq!(ctx.drawing.status(&ctx, &key).phase, Phase::Paused);
    assert!(!ctx.drawing.status(&ctx, &key).enabled);
    let resumed = dispatch(
        &ctx,
        &Request { id: 2, method: rpc::DRAWING_RESUME.into(), params: json!({"key": key}) },
    );
    assert!(resumed.error.is_none(), "{:?}", resumed.error);
    let thread = skwd_wall_core::lock(&ctx.drawing.job).thread.take().unwrap();
    thread.join().unwrap();
    let status = ctx.drawing.status(&ctx, &key);
    assert_eq!(status.phase, Phase::Ready);
    assert!(status.enabled);
    assert!(status.prepared);
}

#[test]
fn pack_keeps_card_choice() {
    let (_guard, _) = crate::testenv::lock();
    let cache_root = tempfile::tempdir().unwrap();
    crate::testenv::write_config(
        json!({"paths": {"cache": cache_root.path()}, "drawing": {"enabled": false}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    preference::save(&ctx, "static:first.webp", true).unwrap();
    let (_directory, manifest) = pack_fixture();
    pack::Pack::inspect(&manifest).unwrap().save(&ctx.state.config()).unwrap();
    let second = ctx.clone();
    let third = ctx.clone();
    let writes = [
        std::thread::spawn(move || preference::save(&second, "static:second.webp", true)),
        std::thread::spawn(move || preference::save(&third, "static:third.webp", false)),
    ];
    for write in writes {
        write.join().unwrap().unwrap();
    }
    let persisted = skwd_wall_core::config::Config::load();
    assert!(skwd_config::drawing_enabled(persisted.root(), "static:first.webp"));
    assert!(skwd_config::drawing_enabled(persisted.root(), "static:second.webp"));
    assert!(!skwd_config::drawing_enabled(persisted.root(), "static:third.webp"));
    assert_eq!(
        skwd_config::str_at(persisted.root(), skwd_config::keys::drawing::PACK, ""),
        manifest.canonicalize().unwrap().to_string_lossy()
    );
}

#[test]
fn repairs_card_cache() {
    let (_guard, root) = crate::testenv::lock();
    let cache_root = tempfile::tempdir().unwrap();
    crate::testenv::write_config(
        json!({"paths": {"cache": cache_root.path()}, "drawing": {"enabled": false}}),
    );
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let (key, source) = item(&ctx, "repair.png");
    let (_directory, manifest) = worker_pack(&root, PREPARES);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());

    assert!(toggle(&ctx, &key, true).error.is_none());
    let thread = skwd_wall_core::lock(&ctx.drawing.job).thread.take().unwrap();
    thread.join().unwrap();
    let artifact = cache::lookup(&ctx.state.config(), &source).unwrap();
    assert!(toggle(&ctx, &key, false).error.is_none());
    std::fs::remove_file(artifact.parent().unwrap().join("strokes.bin")).unwrap();
    assert!(cache::lookup(&ctx.state.config(), &source).is_err());

    assert!(toggle(&ctx, &key, true).error.is_none());
    let thread = skwd_wall_core::lock(&ctx.drawing.job).thread.take().unwrap();
    thread.join().unwrap();
    assert!(cache::lookup(&ctx.state.config(), &source).is_ok());
    assert!(ctx.drawing.status(&ctx, &key).enabled);
}

fn library(names: &[&str]) -> (tempfile::TempDir, serde_json::Value) {
    let cache_root = tempfile::tempdir().unwrap();
    let wallpapers: serde_json::Map<_, _> = names
        .iter()
        .map(|name| (format!("{}{name}", wall_proto::drawing::STATIC_KEY_PREFIX), json!(true)))
        .collect();
    let config = json!({
        "paths": {"cache": cache_root.path()},
        "drawing": {"enabled": false, "autoPrepare": true, "wallpapers": wallpapers}
    });
    (cache_root, config)
}

fn prepare_library(ctx: &Ctx) -> Response {
    dispatch(
        ctx,
        &Request { id: 3, method: rpc::DRAWING_PREPARE.into(), params: json!({"library": true}) },
    )
}

fn finish(ctx: &Ctx) -> Status {
    let thread = skwd_wall_core::lock(&ctx.drawing.job).thread.take().unwrap();
    thread.join().unwrap();
    ctx.drawing.status(ctx, "")
}

fn task(ctx: &Ctx) -> wall_proto::TaskStatus {
    ctx.tasks
        .list()
        .into_iter()
        .find(|task| task.id == wall_proto::drawing::TASK)
        .expect("drawing task")
}

#[test]
fn library_skips_prepared() {
    let (_guard, root) = crate::testenv::lock();
    let names = ["lib-a.png", "lib-b.png", "lib-c.png"];
    let (_cache_root, config) = library(&names);
    crate::testenv::write_config(config);
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    for name in names {
        item(&ctx, name);
    }
    let (worker, manifest) = worker_pack(&root, PREPARES);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());

    assert!(prepare_library(&ctx).error.is_none());
    let status = finish(&ctx);
    assert_eq!((status.completed, status.failed, status.total), (3, 0, 3));
    assert_eq!(task(&ctx).state, wall_proto::TaskState::Completed);

    drop(worker.bin);
    assert!(prepare_library(&ctx).error.is_none());
    let status = finish(&ctx);
    assert_eq!((status.phase, status.completed, status.failed), (Phase::Ready, 3, 0));
}

#[test]
fn library_skips_failures() {
    let (_guard, root) = crate::testenv::lock();
    let names = ["fail-a.png", "fail-b.png"];
    let (_cache_root, config) = library(&names);
    crate::testenv::write_config(config);
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    for name in names {
        item(&ctx, name);
    }
    let (_worker, manifest) = worker_pack(&root, FAILS);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());

    assert!(prepare_library(&ctx).error.is_none());
    let status = finish(&ctx);
    assert_eq!((status.phase, status.completed, status.failed), (Phase::Ready, 0, 2));
}

#[test]
fn pause_holds_automatic() {
    let (_guard, root) = crate::testenv::lock();
    let names = ["hold-a.png", "hold-b.png"];
    let (_cache_root, config) = library(&names);
    crate::testenv::write_config(config);
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    for name in names {
        item(&ctx, name);
    }
    let (_worker, manifest) = worker_pack(&root, PREPARES);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());

    assert!(prepare_library(&ctx).error.is_none());
    assert!(control(&ctx, wall_proto::TaskControl::Pause));
    assert_eq!(task(&ctx).state, wall_proto::TaskState::Paused);
    auto(&ctx);
    assert!(skwd_wall_core::lock(&ctx.drawing.job).activity == Activity::Stopped);
    assert!(control(&ctx, wall_proto::TaskControl::Resume));
    let status = finish(&ctx);
    assert_eq!((status.phase, status.completed), (Phase::Ready, 2));
}

#[test]
fn stop_ends_automatic() {
    let (_guard, root) = crate::testenv::lock();
    let names = ["stop-a.png"];
    let (_cache_root, config) = library(&names);
    crate::testenv::write_config(config);
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    item(&ctx, names[0]);
    let (_worker, manifest) = worker_pack(&root, PREPARES);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());

    auto(&ctx);
    assert!(skwd_wall_core::lock(&ctx.drawing.job).activity != Activity::Stopped);
    assert!(control(&ctx, wall_proto::TaskControl::Stop));
    assert_eq!(task(&ctx).state, wall_proto::TaskState::Cancelled);
    let persisted = skwd_wall_core::config::Config::load();
    assert!(!skwd_config::bool_at(
        persisted.root(),
        skwd_config::keys::drawing::AUTO_PREPARE,
        true
    ));
}

#[test]
fn card_joins_library() {
    let (_guard, root) = crate::testenv::lock();
    let (_cache_root, config) = library(&[]);
    crate::testenv::write_config(config);
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let (key, _) = item(&ctx, "joins.png");
    let (_worker, manifest) = worker_pack(&root, PREPARES);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());
    {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        job.activity = Activity::Waiting;
        job.library = true;
        job.pending = VecDeque::from([String::from("static:queued.png")]);
    }
    assert!(toggle(&ctx, &key, true).error.is_none());
    let mut job = skwd_wall_core::lock(&ctx.drawing.job);
    assert_eq!(job.pending.front(), Some(&key));
    assert!(job.enable.contains(&key));
    job.activity = Activity::Stopped;
    job.library = false;
    job.pending.clear();
}

#[test]
fn paused_library_resumes() {
    let (_guard, root) = crate::testenv::lock();
    let names = ["paused-a.png", "paused-b.png"];
    let (_cache_root, config) = library(&names);
    crate::testenv::write_config(config);
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    for name in names {
        item(&ctx, name);
    }
    let (card, _) = item(&ctx, "paused-card.png");
    let (_worker, manifest) = worker_pack(&root, PREPARES);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());

    assert!(prepare_library(&ctx).error.is_none());
    assert!(control(&ctx, wall_proto::TaskControl::Pause));
    assert!(toggle(&ctx, &card, true).error.is_none());
    finish(&ctx);
    assert_eq!(task(&ctx).state, wall_proto::TaskState::Paused);
    assert!(control(&ctx, wall_proto::TaskControl::Resume));
    let status = finish(&ctx);
    assert_eq!((status.phase, status.completed), (Phase::Ready, 3));
    assert_eq!(task(&ctx).state, wall_proto::TaskState::Completed);
    assert!(!batch::held(&ctx.state.config()));
}

#[test]
fn cards_join_library() {
    let (_guard, root) = crate::testenv::lock();
    let (_cache_root, config) = library(&[]);
    crate::testenv::write_config(config);
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let (first, _) = item(&ctx, "join-a.png");
    let (second, _) = item(&ctx, "join-b.png");
    let (_worker, manifest) = worker_pack(&root, PREPARES);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());
    {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        job.activity = Activity::Waiting;
        job.library = true;
    }
    assert!(toggle(&ctx, &first, true).error.is_none());
    assert!(toggle(&ctx, &second, true).error.is_none());
    assert!(toggle(&ctx, &first, false).error.is_none());
    let mut job = skwd_wall_core::lock(&ctx.drawing.job);
    assert_eq!(job.enable, std::collections::BTreeSet::from([second.clone()]));
    assert_eq!(job.pending, [second]);
    assert!(job.library);
    job.activity = Activity::Stopped;
    job.library = false;
    job.pending.clear();
    job.enable.clear();
}

#[test]
fn automatic_skips_failures() {
    let (_guard, root) = crate::testenv::lock();
    let names = ["retry-a.png", "retry-b.png"];
    let (_cache_root, config) = library(&names);
    crate::testenv::write_config(config);
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    for name in names {
        item(&ctx, name);
    }
    let runs = root.join("bin").join("runs");
    let _ = std::fs::remove_file(&runs);
    let script = format!("#!/bin/sh\necho run >> {}\nexit 1\n", runs.display());
    let (_worker, manifest) = worker_pack(&root, &script);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());
    let count = || std::fs::read_to_string(&runs).unwrap_or_default().lines().count();

    assert!(prepare_library(&ctx).error.is_none());
    finish(&ctx);
    let first = count();
    auto(&ctx);
    let status = finish(&ctx);
    assert_eq!((status.failed, count()), (2, first));
    assert!(prepare_library(&ctx).error.is_none());
    finish(&ctx);
    assert!(count() > first);
}

#[test]
fn library_spares_cards() {
    let (_guard, root) = crate::testenv::lock();
    let (_cache_root, config) = library(&[]);
    crate::testenv::write_config(config);
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let (other, _) = item(&ctx, "spare.png");
    let (_worker, manifest) = worker_pack(&root, PREPARES);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());
    {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        job.library = true;
        job.status.phase = Phase::Preparing;
        job.status.active_key = String::from("static:busy.png");
    }
    assert_eq!(ctx.drawing.status(&ctx, &other).phase, Phase::Idle);
    let mut job = skwd_wall_core::lock(&ctx.drawing.job);
    job.library = false;
    job.status = Status::default();
}

#[test]
fn failed_card_stays_disabled() {
    let (_guard, root) = crate::testenv::lock();
    let (_cache_root, config) = library(&[]);
    crate::testenv::write_config(config);
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let (card, _) = item(&ctx, "broken-card.png");
    let (_worker, manifest) = worker_pack(&root, FAILS);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());
    {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        job.library = true;
        job.pending = VecDeque::from([card.clone()]);
        job.enable.insert(card.clone());
        job.spawn(&ctx, pack::Pack::load(&ctx.state.config()).unwrap());
    }
    let status = finish(&ctx);
    assert_eq!(status.failed, 1);
    assert!(skwd_wall_core::lock(&ctx.drawing.job).enable.is_empty());
    assert!(!skwd_config::drawing_enabled(skwd_wall_core::config::Config::load().root(), &card));
}

#[test]
fn card_pause_spares_library() {
    let (_guard, root) = crate::testenv::lock();
    let names = ["spare-a.png"];
    let (_cache_root, config) = library(&names);
    crate::testenv::write_config(config);
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    item(&ctx, names[0]);
    let (card, _) = item(&ctx, "spare-card.png");
    let (_worker, manifest) = worker_pack(&root, PREPARES);
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());

    assert!(prepare_library(&ctx).error.is_none());
    assert!(control(&ctx, wall_proto::TaskControl::Pause));
    assert!(toggle(&ctx, &card, true).error.is_none());
    let cancel =
        dispatch(&ctx, &Request { id: 4, method: rpc::DRAWING_CANCEL.into(), params: json!({}) });
    assert!(cancel.error.is_none());
    assert!(batch::held(&ctx.state.config()));
    let persisted = skwd_wall_core::config::Config::load();
    assert!(skwd_config::bool_at(
        persisted.root(),
        skwd_config::keys::drawing::AUTO_PREPARE,
        false
    ));
    assert!(control(&ctx, wall_proto::TaskControl::Stop));
    assert!(!batch::held(&ctx.state.config()));
}

#[test]
fn tidy_waits_for_worker() {
    let (_guard, _) = crate::testenv::lock();
    let (_cache_root, config) = library(&[]);
    crate::testenv::write_config(config);
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    item(&ctx, "tidy.png");
    let leftover = cache::root(&ctx.state.config()).join(cache::WORK_DIRECTORY).join("job-left");
    std::fs::create_dir_all(&leftover).unwrap();
    {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        job.activity = Activity::Waiting;
        job.library = true;
    }
    tidy(&ctx);
    assert!(leftover.exists());
    {
        let mut job = skwd_wall_core::lock(&ctx.drawing.job);
        assert!(job.plan);
        job.activity = Activity::Stopped;
        job.library = false;
        job.plan = false;
    }
    tidy(&ctx);
    assert!(!leftover.exists());
}

#[test]
fn widget_depth_plans() {
    for (depth, planned) in [("behind", true), ("front", false)] {
        let (_guard, _) = crate::testenv::lock();
        let (_cache_root, mut config) = library(&[]);
        config["widgets"] = json!({"items": [{
            "face": {"kind": "clock", "style": "aperture"},
            "depth": depth,
            "pin": {"anchor": [0.5, 0.5], "offset": [0.0, 0.0]},
        }]});
        crate::testenv::write_config(config);
        let (state, events, stats) = crate::testenv::harness();
        let ctx = crate::testenv::context(&state, &events, &stats);
        let (key, _) = item(&ctx, "outlined.png");
        assert_eq!(
            super::batch::plan(&ctx, "pack").unwrap().keys.contains(&key),
            planned,
            "{depth}"
        );
    }
}

const GREEDY: &str = r#"device = args[args.index("--device") + 1]
with open(pathlib.Path(sys.argv[0]).with_name("devices"), "a") as log:
    log.write(device + "\n")
if device != "cpu":
    hold = b"x" * (640 << 20)
    time.sleep(30)
"#;

struct Greedy {
    ctx: Ctx,
    devices: PathBuf,
    _cache: tempfile::TempDir,
    _worker: Worker,
}

fn greedy(root: &Path, device: &str) -> Greedy {
    let cache = tempfile::tempdir().unwrap();
    crate::testenv::write_config(json!({
        "paths": {"cache": cache.path()},
        "drawing": {"enabled": false, "device": device, "memoryLimitMib": 512}
    }));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let devices = root.join("bin").join("devices");
    let _ = std::fs::remove_file(&devices);
    let (worker, manifest) = worker_pack(root, &PREPARES.replace("time.sleep(0.3)\n", GREEDY));
    let body = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, body.replace(r#""device":"cpu""#, r#""device":"auto""#)).unwrap();
    save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());
    Greedy { ctx, devices, _cache: cache, _worker: worker }
}

fn still(ctx: &Ctx, name: &str) -> String {
    let (key, path) = item(ctx, name);
    image::RgbImage::new(8, 8).save_with_format(&path, image::ImageFormat::Png).unwrap();
    key
}

fn prepare_card(ctx: &Ctx, key: &str) -> Status {
    assert!(toggle(ctx, key, true).error.is_none());
    let thread = skwd_wall_core::lock(&ctx.drawing.job).thread.take().unwrap();
    thread.join().unwrap();
    ctx.drawing.status(ctx, key)
}

#[test]
fn memory_kill_retries_cpu() {
    let (_guard, root) = crate::testenv::lock();
    let Greedy { ctx, devices, .. } = &greedy(&root, "auto");
    let first = still(ctx, "greedy-a.png");
    let status = prepare_card(ctx, &first);
    assert!(status.prepared, "{:?}", status.error);
    assert_eq!(skwd_wall_core::lock(&ctx.drawing.job).cpu_from.map(|from| from.pixels), Some(64));
    let second = still(ctx, "greedy-b.png");
    assert!(prepare_card(ctx, &second).prepared);
    assert_eq!(std::fs::read_to_string(devices).unwrap(), "auto\ncpu\ncpu\n");
}

#[test]
fn raised_limit_retries_auto() {
    let (_guard, root) = crate::testenv::lock();
    let Greedy { ctx, devices, _cache: cache, .. } = &greedy(&root, "auto");
    assert!(prepare_card(ctx, &still(ctx, "greedy-d.png")).prepared);
    assert!(skwd_wall_core::lock(&ctx.drawing.job).cpu_from.is_some());
    crate::testenv::write_config(json!({
        "paths": {"cache": cache.path()},
        "drawing": {"enabled": false, "device": "auto", "memoryLimitMib": 4096}
    }));
    ctx.state.reload_config();
    let key = still(ctx, "greedy-e.png");
    assert!(toggle(ctx, &key, true).error.is_none());
    let tried = || std::fs::read_to_string(devices).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while tried() != "auto\ncpu\nauto\n" {
        assert!(std::time::Instant::now() < deadline, "no retry on auto");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    ctx.drawing.cancel(ctx, Phase::Cancelled);
}

#[test]
fn invalid_settings_warn_once() {
    let (_guard, root) = crate::testenv::lock();
    let Greedy { ctx, .. } = &greedy(&root, "rocm");
    let config = ctx.state.config().clone();
    run::warn_invalid_settings(ctx, &config);
    run::warn_invalid_settings(ctx, &config);
    assert_eq!(skwd_wall_core::lock(&ctx.drawing.job).warned.len(), 1);
}

#[test]
fn chosen_device_no_retry() {
    let (_guard, root) = crate::testenv::lock();
    let Greedy { ctx, devices, .. } = &greedy(&root, "cuda");
    let key = still(ctx, "greedy-c.png");
    let status = prepare_card(ctx, &key);
    assert_eq!(status.phase, Phase::Failed);
    assert!(status.error.unwrap().contains("memory limit"));
    assert_eq!(std::fs::read_to_string(devices).unwrap(), "cuda\n");
    assert!(skwd_wall_core::lock(&ctx.drawing.job).cpu_from.is_none());
}

const RECORDS_GPU: &str = r#"import os
with open(pathlib.Path(sys.argv[0]).with_name("gpu"), "w") as log:
    log.write(os.environ["SKWD_VK_DEVICE"])
"#;

#[test]
fn passes_configured_gpu() {
    let (_guard, root) = crate::testenv::lock();
    let (_worker, manifest) =
        worker_pack(&root, &PREPARES.replace("time.sleep(0.3)\n", RECORDS_GPU));
    for (setting, passed) in [
        (json!("uuid:11D5340ACA29CD2FF52256BCD66AB356"), "uuid:11d5340aca29cd2ff52256bcd66ab356"),
        (json!(null), "auto"),
    ] {
        let cache = tempfile::tempdir().unwrap();
        crate::testenv::write_config(json!({
            "paths": {"cache": cache.path()},
            "drawing": {"enabled": false},
            "performance": {"gpuDevice": setting}
        }));
        let (state, events, stats) = crate::testenv::harness();
        let ctx = crate::testenv::context(&state, &events, &stats);
        save_pack(&ctx.state.config(), &pack::Pack::inspect(&manifest).unwrap());
        assert!(prepare_card(&ctx, &still(&ctx, "gpu.png")).prepared);
        let recorded = std::fs::read_to_string(root.join("bin").join("gpu")).unwrap();
        assert_eq!(recorded, passed);
    }
}
