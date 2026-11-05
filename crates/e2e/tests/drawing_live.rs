use serde_json::{Value, json};
use skwd_e2e::{Client, Sandbox, Walld, ffmpeg_still, wait_until};
use skwd_wall_core::infrastructure::drawing;
use std::time::Duration;

const KEY: &str = "static:scene.png";
const PACK_ENV: &str = "SKWD_TEST_DRAWING_PACK";
const HOME_ENV: &str = "SKWD_DRAWING_HOME";
const READY_TIMEOUT: Duration = Duration::from_secs(120);

fn pack_ready(client: &mut Client, id: u64) -> bool {
    let status =
        client.call(wall_proto::rpc::DRAWING_STATUS, json!({}), id).unwrap()["result"].clone();
    status["pack"] == "available" && status["phase"] != "checking"
}

fn status(client: &mut Client, id: u64) -> Value {
    client.call(wall_proto::rpc::DRAWING_STATUS, json!({ "key": KEY }), id).unwrap()["result"]
        .clone()
}

#[test]
#[ignore = "requires release daemon and a drawing pack"]
fn native_helper_prepares() {
    let pack = std::env::var(PACK_ENV).expect("drawing pack manifest");
    let sandbox = Sandbox::new("drawing-live");
    assert!(ffmpeg_still(&sandbox.library().join("scene.png"), "testsrc2=s=1280x720"));
    sandbox.write_config(&json!({
        "paths": {"wallpaper": sandbox.library(), "videoWallpaper": sandbox.library()},
        "pickOnlyMode": true,
        "restoreOnStartup": false
    }));
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    assert!(wait_until(
        || client
            .call(wall_proto::rpc::WALL_LIST, json!({}), 1)
            .and_then(|response| response["result"]["wallpapers"].as_array().cloned())
            .is_some_and(|rows| rows.iter().any(|row| row["key"] == KEY)),
        Duration::from_secs(15)
    ));
    let registered =
        client.call(wall_proto::rpc::DRAWING_REGISTER, json!({ "path": pack }), 2).unwrap();
    assert!(registered.get("error").is_none(), "{registered}");
    assert!(wait_until(|| pack_ready(&mut client, 3), READY_TIMEOUT));
    let prepared = client.call(wall_proto::rpc::DRAWING_PREPARE, json!({ "key": KEY }), 4).unwrap();
    assert!(prepared.get("error").is_none(), "{prepared}");
    assert!(wait_until(
        || matches!(status(&mut client, 5)["phase"].as_str(), Some("ready" | "failed")),
        Duration::from_secs(180)
    ));
    let finished = status(&mut client, 6);
    assert_eq!(finished["phase"], "ready", "{finished}");
    assert_eq!(finished["prepared"], true);
    assert_eq!(finished["stage"], wall_proto::drawing::stage::COMPLETE);
}

#[test]
#[ignore = "requires release daemon and a drawing pack"]
fn system_pack_adopts() {
    let pack = std::path::PathBuf::from(std::env::var(PACK_ENV).expect("drawing pack manifest"));
    let home = pack.parent().expect("drawing pack directory");
    let mut sandbox = Sandbox::new("drawing-system");
    sandbox.set_env(HOME_ENV, home.to_str().expect("drawing home path"));
    assert!(ffmpeg_still(&sandbox.library().join("scene.png"), "testsrc2=s=1280x720"));
    sandbox.write_config(&json!({
        "paths": {"wallpaper": sandbox.library(), "videoWallpaper": sandbox.library()},
        "pickOnlyMode": true,
        "restoreOnStartup": false
    }));
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    assert!(wait_until(|| pack_ready(&mut client, 1), READY_TIMEOUT), "{}", walld.log_contents());
    let prepared = client.call(wall_proto::rpc::DRAWING_PREPARE, json!({ "key": KEY }), 2).unwrap();
    assert!(prepared.get("error").is_none(), "{prepared}");
    assert!(wait_until(
        || matches!(status(&mut client, 3)["phase"].as_str(), Some("ready" | "failed")),
        Duration::from_secs(180)
    ));
    let finished = status(&mut client, 4);
    assert_eq!(finished["phase"], "ready", "{finished}");
    assert_eq!(finished["prepared"], true);
}

const PAPER_ENV: &str = "SKWD_TEST_PAPER_BIN";
const WE_ASSETS_ENV: &str = "SKWD_TEST_WE_ASSETS";
const WE_ITEMS_ENV: &str = "SKWD_TEST_WE_ITEMS";
const PATTERNS: [&str; 6] = [
    "testsrc2=s=1280x720",
    "mandelbrot=s=1280x720",
    "smptebars=s=1280x720",
    "rgbtestsrc=s=1280x720",
    "cellauto=s=1280x720",
    "life=s=1280x720:mold=10:ratio=0.5",
];
const RGB: u8 = 2;
const GRAY_ALPHA: u8 = 4;
const RIFF_MAGIC: &[u8] = b"RIFF";
const WEBP_MAGIC: &[u8] = b"WEBP";
const WEBP_MAGIC_AT: std::ops::Range<usize> = 8..12;
const PREVIEW_EDGE: u32 = 320;

fn layout(sandbox: &Sandbox) -> skwd_wall_core::config::Config {
    skwd_wall_core::config::Config::from_root(json!({"paths": {"cache": sandbox.cache()}}))
}

fn cached(sandbox: &Sandbox, directory: &str) -> std::path::PathBuf {
    drawing::root(&layout(sandbox)).join(directory)
}

fn stills(sandbox: &Sandbox, names: &[&str]) {
    for (name, pattern) in names.iter().zip(PATTERNS.iter().cycle()) {
        assert!(ffmpeg_still(&sandbox.library().join(name), pattern), "{name}");
    }
}

fn library_ready(client: &mut Client, count: usize) -> bool {
    wait_until(
        || {
            client
                .call(wall_proto::rpc::WALL_LIST, json!({}), 1)
                .and_then(|response| response["result"]["wallpapers"].as_array().cloned())
                .is_some_and(|rows| rows.len() >= count)
        },
        Duration::from_secs(30),
    )
}

fn register(client: &mut Client) {
    let pack = std::env::var(PACK_ENV).expect("drawing pack manifest");
    let registered =
        client.call(wall_proto::rpc::DRAWING_REGISTER, json!({ "path": pack }), 2).unwrap();
    assert!(registered.get("error").is_none(), "{registered}");
    assert!(wait_until(|| pack_ready(client, 3), READY_TIMEOUT));
}

fn task(client: &mut Client) -> Value {
    client
        .call(wall_proto::rpc::TASK_LIST, json!({}), 4)
        .and_then(|response| response["result"]["tasks"].as_array().cloned())
        .and_then(|tasks| tasks.into_iter().find(|task| task["id"] == wall_proto::drawing::TASK))
        .unwrap_or(Value::Null)
}

fn control(client: &mut Client, action: &str) -> Value {
    client
        .call(
            wall_proto::rpc::TASK_CONTROL,
            json!({"id": wall_proto::drawing::TASK, "action": action}),
            5,
        )
        .unwrap()
}

fn prepare_all(client: &mut Client) {
    let started =
        client.call(wall_proto::rpc::DRAWING_PREPARE, json!({"library": true}), 6).unwrap();
    assert!(started.get("error").is_none(), "{started}");
}

fn wait_state(client: &mut Client, state: &str, timeout: Duration) -> Value {
    assert!(wait_until(|| task(client)["state"] == state, timeout), "{}", task(client));
    task(client)
}

fn entries(directory: &std::path::Path) -> Vec<(String, std::time::SystemTime)> {
    let mut found: Vec<_> = std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| {
            let modified = entry.metadata().unwrap().modified().unwrap();
            (entry.file_name().to_string_lossy().into_owned(), modified)
        })
        .collect();
    found.sort();
    found
}

fn png_header(path: &std::path::Path) -> (u32, u32, u8) {
    let bytes = std::fs::read(path).unwrap();
    let number = |offset: usize| u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap());
    (number(16), number(20), bytes[25])
}

fn assert_compact(sandbox: &Sandbox) {
    let prepared = drawing::artifacts(&layout(sandbox));
    for (name, _) in entries(&prepared) {
        let directory = prepared.join(name);
        let manifest: paper_control::DrawingManifest =
            serde_json::from_slice(&std::fs::read(directory.join(drawing::MANIFEST)).unwrap())
                .unwrap();
        assert_eq!(png_header(&directory.join(&manifest.texture)).2, RGB);
        let strokes = paper_control::DrawingStroke::read(&directory.join(&manifest.strokes));
        assert!(!strokes.unwrap().is_empty(), "planned strokes");
        let (width, height, kind) = png_header(&directory.join(drawing::PREVIEW));
        assert_eq!(kind, GRAY_ALPHA);
        assert!(width.max(height) <= PREVIEW_EDGE, "{width}x{height}");
    }
}

fn drawing_config(sandbox: &Sandbox, automatic: bool, extra: &Value) {
    let empty = sandbox.root.join("no-workshop");
    std::fs::create_dir_all(&empty).unwrap();
    let mut config = json!({
        "paths": {
            "wallpaper": sandbox.library(),
            "videoWallpaper": sandbox.library(),
            "steamWorkshop": empty,
            "paperBin": sandbox.root.join("no-paper")
        },
        "pickOnlyMode": true,
        "restoreOnStartup": false,
        "drawing": {"enabled": true, "autoPrepare": automatic}
    });
    if let (Some(paths), Some(more)) = (config["paths"].as_object_mut(), extra.as_object()) {
        paths.extend(more.clone());
    }
    sandbox.write_config(&config);
}

#[test]
#[ignore = "requires release daemon and a drawing pack"]
fn library_resumes_after_restart() {
    let names = ["a.png", "b.png", "c.png", "d.png", "e.png", "f.png"];
    let sandbox = Sandbox::new("drawing-library-restart");
    stills(&sandbox, &names);
    drawing_config(&sandbox, false, &json!({}));
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    assert!(library_ready(&mut client, names.len()));
    register(&mut client);
    prepare_all(&mut client);
    assert!(wait_until(
        || task(&mut client)["progress"].as_u64().unwrap_or(0) >= 2,
        Duration::from_secs(240)
    ));
    assert_eq!(control(&mut client, "pause")["result"]["ok"], true);
    let paused = wait_state(&mut client, "paused", Duration::from_secs(30));
    assert!(cached(&sandbox, drawing::PAUSE_MARKER).exists());
    let before = entries(&cached(&sandbox, drawing::INDEX_DIRECTORY));
    assert!(before.len() >= 2 && before.len() < names.len(), "{paused}");
    drop(walld);

    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    assert!(library_ready(&mut client, names.len()));
    wait_state(&mut client, "paused", Duration::from_secs(30));
    assert_eq!(control(&mut client, "resume")["result"]["ok"], true);
    let done = wait_state(&mut client, "completed", Duration::from_secs(400));
    assert_eq!((done["progress"].as_u64(), done["total"].as_u64()), (Some(6), Some(6)));
    let status = client.call(wall_proto::rpc::DRAWING_STATUS, json!({}), 7).unwrap();
    assert_eq!(status["result"]["failed"], 0);
    let after = entries(&cached(&sandbox, drawing::INDEX_DIRECTORY));
    assert_eq!(after.len(), names.len());
    assert!(before.iter().all(|entry| after.contains(entry)), "prepared again");
    assert!(!cached(&sandbox, drawing::PAUSE_MARKER).exists());
    assert_compact(&sandbox);
}

#[test]
#[ignore = "requires release daemon and a drawing pack"]
fn automatic_prepares_new() {
    let sandbox = Sandbox::new("drawing-automatic");
    stills(&sandbox, &["first.png", "second.png"]);
    drawing_config(&sandbox, true, &json!({}));
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    assert!(library_ready(&mut client, 2));
    register(&mut client);
    stills(&sandbox, &["third.png"]);
    let done = wait_state(&mut client, "completed", Duration::from_secs(300));
    assert_eq!(done["total"].as_u64(), Some(3));
    assert_eq!(entries(&drawing::artifacts(&layout(&sandbox))).len(), 3);

    stills(&sandbox, &["fourth.png", "fifth.png", "sixth.png"]);
    wait_state(&mut client, "running", Duration::from_secs(60));
    assert_eq!(control(&mut client, "stop")["result"]["ok"], true);
    wait_state(&mut client, "cancelled", Duration::from_secs(60));
    let config: Value =
        serde_json::from_str(&std::fs::read_to_string(sandbox.config_path()).unwrap()).unwrap();
    assert_eq!(config["drawing"]["autoPrepare"], false);
    stills(&sandbox, &["seventh.png"]);
    assert!(library_ready(&mut client, 7));
    std::thread::sleep(Duration::from_secs(5));
    assert_eq!(task(&mut client)["state"], "cancelled");
}

#[test]
#[ignore = "requires release daemon and a drawing pack"]
fn crash_resumes_and_cleans() {
    let names = ["one.png", "two.png", "three.png", "four.png"];
    let sandbox = Sandbox::new("drawing-crash");
    stills(&sandbox, &names);
    drawing_config(&sandbox, false, &json!({}));
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    assert!(library_ready(&mut client, names.len()));
    register(&mut client);
    prepare_all(&mut client);
    let helper = |pid: u32| skwd_e2e::child_pids(pid, "skwd-wall-draw");
    assert!(wait_until(
        || task(&mut client)["progress"].as_u64().unwrap_or(0) >= 2
            && !helper(walld.pid()).is_empty(),
        Duration::from_secs(240)
    ));
    let orphans = helper(walld.pid());
    let finished = entries(&cached(&sandbox, drawing::INDEX_DIRECTORY));
    drop(walld);
    assert!(
        wait_until(|| orphans.iter().all(|&pid| skwd_e2e::exited(pid)), Duration::from_secs(120)),
        "{:?}",
        orphans
            .iter()
            .map(|pid| std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default())
            .collect::<Vec<_>>()
    );

    assert!(!finished.is_empty());
    assert!(!entries(&cached(&sandbox, drawing::WORK_DIRECTORY)).is_empty());
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    assert!(library_ready(&mut client, names.len()));
    prepare_all(&mut client);
    let done = wait_state(&mut client, "completed", Duration::from_secs(400));
    assert_eq!(done["total"].as_u64(), Some(4));
    let index = entries(&cached(&sandbox, drawing::INDEX_DIRECTORY));
    assert_eq!(index.len(), names.len());
    assert!(finished.iter().all(|entry| index.contains(entry)), "prepared again");
    assert!(entries(&cached(&sandbox, drawing::WORK_DIRECTORY)).is_empty());
}

#[test]
#[ignore = "requires release daemon, a drawing pack, Paper binaries and Wallpaper Engine items"]
fn media_references_compact() {
    let paper = std::env::var(PAPER_ENV).expect("Paper binary");
    let assets = std::env::var(WE_ASSETS_ENV).expect("Wallpaper Engine assets");
    let items = std::env::var(WE_ITEMS_ENV).expect("Wallpaper Engine item directories");
    let sandbox = Sandbox::new("drawing-media");
    let workshop = sandbox.root.join("workshop");
    std::fs::create_dir_all(&workshop).unwrap();
    let mut count = 1;
    for item in items.split(':') {
        let source = std::path::Path::new(item);
        let target = workshop.join(source.file_name().unwrap());
        assert!(
            std::process::Command::new("cp")
                .arg("-r")
                .arg(source)
                .arg(&target)
                .status()
                .unwrap()
                .success()
        );
        count += 1;
    }
    assert!(skwd_e2e::ffmpeg_video(&sandbox.library().join("clip.mp4"), "teal", 2.0));
    stills(&sandbox, &["kept.png"]);
    drawing_config(
        &sandbox,
        true,
        &json!({"paperBin": paper, "steamWorkshop": workshop, "steamWeAssets": assets}),
    );
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    assert!(library_ready(&mut client, count + 1));
    register(&mut client);
    prepare_all(&mut client);
    let done = wait_state(&mut client, "completed", Duration::from_secs(900));
    let status = client.call(wall_proto::rpc::DRAWING_STATUS, json!({}), 8).unwrap();
    assert_eq!(status["result"]["failed"], 0, "{done}\n{}", walld.log_contents());
    let reference_root = drawing::references(&layout(&sandbox));
    let references = entries(&reference_root);
    assert_eq!(references.len(), count, "{references:?}");
    for (name, _) in &references {
        let path = reference_root.join(name);
        let bytes = std::fs::read(&path).unwrap();
        eprintln!("reference {name}: {} bytes", bytes.len());
        assert!(
            bytes.starts_with(RIFF_MAGIC) && bytes.get(WEBP_MAGIC_AT) == Some(WEBP_MAGIC),
            "{name}"
        );
    }
    assert_compact(&sandbox);

    std::fs::remove_file(sandbox.library().join("clip.mp4")).unwrap();
    assert!(wait_until(|| entries(&reference_root).len() == count - 1, Duration::from_secs(120)));
    assert_eq!(entries(&cached(&sandbox, drawing::MEDIA_DIRECTORY)).len(), count - 1);
}

#[test]
#[ignore = "requires release daemon and a drawing pack"]
fn planning_scales() {
    const COUNT: usize = 1500;
    let sandbox = Sandbox::new("drawing-scale");
    let seed = sandbox.root.join("seed.png");
    assert!(ffmpeg_still(&seed, "testsrc2=s=64x36"));
    for index in 0..COUNT {
        std::fs::copy(&seed, sandbox.library().join(format!("{index:04}.png"))).unwrap();
    }
    drawing_config(&sandbox, false, &json!({}));
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    assert!(wait_until(
        || client
            .call(wall_proto::rpc::WALL_LIST, json!({}), 1)
            .and_then(|response| response["result"]["wallpapers"].as_array().map(Vec::len))
            .is_some_and(|rows| rows >= COUNT),
        Duration::from_secs(600)
    ));
    register(&mut client);
    let started = std::time::Instant::now();
    prepare_all(&mut client);
    let mut slowest = Duration::ZERO;
    assert!(wait_until(
        || {
            let asked = std::time::Instant::now();
            client.call(wall_proto::rpc::DRAWING_STATUS, json!({}), 9).unwrap();
            slowest = slowest.max(asked.elapsed());
            task(&mut client)["total"].as_u64().unwrap_or(0) >= COUNT as u64
        },
        Duration::from_secs(120)
    ));
    let planned = started.elapsed();
    assert_eq!(control(&mut client, "stop")["result"]["ok"], true);
    wait_state(&mut client, "cancelled", Duration::from_secs(60));
    seed_prepared(&sandbox, COUNT);
    let started = std::time::Instant::now();
    prepare_all(&mut client);
    let done = wait_state(&mut client, "completed", Duration::from_secs(120));
    let recognised = started.elapsed();
    eprintln!("cold plan {planned:?}, warm {recognised:?}, slowest status {slowest:?}");
    assert_eq!(done["progress"].as_u64(), Some(COUNT as u64));
    assert_eq!(done["total"].as_u64(), Some(COUNT as u64));
    assert!(planned < Duration::from_secs(30) && recognised < Duration::from_secs(30));
    assert!(slowest < Duration::from_secs(2));
}

fn seed_prepared(sandbox: &Sandbox, count: usize) {
    let config = layout(sandbox);
    let pack: Value = serde_json::from_slice(
        &std::fs::read(drawing::root(&config).join(drawing::PACK_RECORD)).unwrap(),
    )
    .unwrap();
    for index in 0..count {
        let source = sandbox.library().join(format!("{index:04}.png")).canonicalize().unwrap();
        let artifact = drawing::digest(source.as_os_str().as_encoded_bytes());
        let directory = drawing::artifacts(&config).join(&artifact);
        std::fs::create_dir_all(&directory).unwrap();
        let metadata = source.metadata().unwrap();
        let modified = metadata.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap();
        let manifest = json!({
            "version": 3,
            "source": {
                "path": source,
                "size": metadata.len(),
                "mtime_ns": modified.as_nanos(),
                "sha256": drawing::digest(b"seed")
            },
            "width": 64, "height": 36,
            "texture": "drawing.png", "strokes": "strokes.bin", "timing_width": 8, "timing_height": 8, "focus": [0.5, 0.5]
        });
        std::fs::write(directory.join(drawing::MANIFEST), manifest.to_string()).unwrap();
        let entry = drawing::CacheEntry {
            artifact,
            pack: pack["fingerprint"].as_str().unwrap().to_string(),
            recipe: drawing::RECIPE.to_string(),
        };
        drawing::publish(&config, &source, &entry).unwrap();
    }
}
