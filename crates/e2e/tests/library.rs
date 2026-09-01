use serde_json::{Value, json};
use skwd_e2e::{
    Checks, Client, Sandbox, Walld, db_count, ffmpeg_still, ffmpeg_video, pss_mb, scan_pids,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const FOLDERS: [&str; 7] = [
    "",
    "anime/seasonal/2024",
    "anime/movies",
    "photos",
    "with space",
    "unicode-桜と海",
    "deep/a/b/c/d",
];

const STATICS: usize = 160;
const MASS_BATCH: usize = 520;

struct Item {
    kind: String,
    path: PathBuf,
    thumb: String,
}

fn list_items(client: &mut Client, lib: &Path) -> Vec<Item> {
    let resp = client.call("wall.list", json!({}), 1);
    let rows = resp
        .as_ref()
        .and_then(|value| value.get("result")?.get("wallpapers")?.as_array())
        .cloned()
        .unwrap_or_default();
    rows.iter()
        .map(|row| {
            let key = row.get("key").and_then(Value::as_str).unwrap_or("");
            let (kind, rel) = key.split_once(':').unwrap_or(("static", key));
            Item {
                kind: if kind.is_empty() { "static".into() } else { kind.to_string() },
                path: if rel.is_empty() { PathBuf::new() } else { lib.join(rel) },
                thumb: row.get("thumb").and_then(Value::as_str).unwrap_or("").to_string(),
            }
        })
        .collect()
}

fn list_count(client: &mut Client, lib: &Path) -> usize {
    list_items(client, lib)
        .iter()
        .filter(|item| item.kind == "static" || item.kind == "video")
        .count()
}

fn wait_count(
    client: &mut Client,
    lib: &Path,
    target: usize,
    walld_pid: u32,
    timeout: Duration,
) -> (f64, f64, usize) {
    let start = Instant::now();
    let mut peak_scan = 0.0f64;
    let mut last = usize::MAX;
    while start.elapsed() < timeout {
        for scan in scan_pids(walld_pid) {
            peak_scan = peak_scan.max(pss_mb(scan));
        }
        last = list_count(client, lib);
        if last == target {
            break;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    (start.elapsed().as_secs_f64(), peak_scan, last)
}

fn copy_batch(
    sources: &[PathBuf],
    lib: &Path,
    start: usize,
    count: usize,
    prefix: &str,
) -> Vec<PathBuf> {
    let mut copied = Vec::with_capacity(count);
    for offset in 0..count {
        let idx = start + offset;
        let src = &sources[idx % sources.len()];
        let dst_dir = lib.join(FOLDERS[idx % FOLDERS.len()]);
        fs::create_dir_all(&dst_dir).expect("mkdir batch");
        let ext = src.extension().and_then(|value| value.to_str()).unwrap_or("png");
        let dst = dst_dir.join(format!("{prefix}{idx:04}.{ext}"));
        fs::copy(src, &dst).expect("copy fixture");
        copied.push(dst);
    }
    copied
}

fn make_sources(dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    fs::create_dir_all(dir).expect("sources dir");
    let statics = ["red", "green", "blue", "gray"]
        .iter()
        .enumerate()
        .filter_map(|(idx, color)| {
            let dest = dir.join(format!("src{idx}.png"));
            ffmpeg_still(&dest, &format!("color=c={color}:s=320x180")).then_some(dest)
        })
        .collect();
    let videos = ["yellow", "magenta"]
        .iter()
        .enumerate()
        .filter_map(|(idx, color)| {
            let dest = dir.join(format!("src{idx}.mp4"));
            ffmpeg_video(&dest, color, 1.0).then_some(dest)
        })
        .collect();
    (statics, videos)
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn library_scan_watch_move_delete() {
    let mut sandbox = Sandbox::new("library");
    let lib = sandbox.library();
    let sqlite = sandbox.sqlite_path();

    let (static_src, video_src) = make_sources(&sandbox.root.join("sources"));
    let have_videos = !video_src.is_empty();
    let videos = if have_videos { 4 } else { 0 };

    let mut checks = Checks::default();
    checks.check("static fixtures generated", static_src.len() == 4, || {
        format!("{} images (ffmpeg missing?)", static_src.len())
    });
    assert!(!static_src.is_empty(), "no static fixtures");
    if !have_videos {
        eprintln!("  note  no video encoder (libx264); video checks skipped");
    }

    let lib_str = lib.to_string_lossy().into_owned();
    let we_empty = sandbox.root.join("we-empty");
    fs::create_dir_all(&we_empty).expect("we dir");
    let we_str = we_empty.to_string_lossy().into_owned();
    sandbox.write_config(&json!({
        "paths": {
            "wallpaper": lib_str,
            "videoWallpaper": lib_str,
            "steamWorkshop": we_str,
            "steamWeAssets": we_str,
        },
        "pickOnlyMode": true,
        "restoreOnStartup": false,
        "general": { "randomInterval": 0 },
        "effects": { "autoRecolor": false, "autoTheme": "" },
    }));

    copy_batch(&static_src, &lib, 0, STATICS, "w");
    let vid_paths =
        if have_videos { copy_batch(&video_src, &lib, 0, videos, "v") } else { Vec::new() };
    let mut total = STATICS + videos;

    let walld = Walld::start(&sandbox);
    let walld_pid = walld.pid();
    let mut client = walld.client();

    let (dt, scan_peak, got) =
        wait_count(&mut client, &lib, total, walld_pid, Duration::from_secs(300));
    checks.check("cold import completes", got == total, || format!("{got}/{total} after {dt:.1}s"));
    eprintln!(
        "  info  cold import {got}/{total} in {dt:.1}s, walld {:.1} MB PSS, scan peak {scan_peak:.1} MB",
        pss_mb(walld_pid)
    );
    checks.check("db matches list", db_count(&sqlite) == got as i64, || {
        format!("db={} list={got}", db_count(&sqlite))
    });
    let video_kind =
        list_items(&mut client, &lib).iter().filter(|item| item.kind == "video").count();
    checks.check("video kind count", video_kind == videos, || format!("{video_kind} != {videos}"));
    let folders_seen: Vec<String> = list_items(&mut client, &lib)
        .iter()
        .filter(|item| !item.path.as_os_str().is_empty())
        .filter_map(|item| {
            item.path
                .parent()?
                .strip_prefix(&lib)
                .ok()
                .map(|rel| rel.to_string_lossy().into_owned())
        })
        .collect();
    checks.check(
        "nested/unicode/space folders imported",
        FOLDERS
            .iter()
            .filter(|folder| !folder.is_empty())
            .all(|folder| folders_seen.iter().any(|seen| seen == folder)),
        || format!("{folders_seen:?}"),
    );

    let before = list_count(&mut client, &lib);
    copy_batch(&static_src, &lib, 1000 + total, 1, "b");
    let (dt, _, got) =
        wait_count(&mut client, &lib, before + 1, walld_pid, Duration::from_secs(60));
    total = if got > 0 { got } else { total };
    checks.check("live watcher single-file import", got == before + 1, || format!("{dt:.1}s"));

    let before = list_count(&mut client, &lib);
    copy_batch(&static_src, &lib, 2000 + total, 20, "b");
    let (dt, _, got) =
        wait_count(&mut client, &lib, before + 20, walld_pid, Duration::from_secs(90));
    checks
        .check("live watcher small-batch import (+20)", got == before + 20, || format!("{dt:.1}s"));

    let before = list_count(&mut client, &lib);
    copy_batch(&static_src, &lib, 5000, MASS_BATCH, "m");
    let (dt, _, got) =
        wait_count(&mut client, &lib, before + MASS_BATCH, walld_pid, Duration::from_secs(300));
    checks.check("mass import >=512 triggers full-rescan path", got == before + MASS_BATCH, || {
        format!("{dt:.1}s")
    });
    checks.check(
        "mass change logged as full rescan",
        walld.log_contents().contains("full rescan"),
        || "expected 'full rescan' in log".into(),
    );
    total = got;
    eprintln!(
        "  info  mass +{MASS_BATCH} settled in {dt:.1}s, walld {:.1} MB PSS",
        pss_mb(walld_pid)
    );

    let move_dir = lib.join("moved/into/here");
    fs::create_dir_all(&move_dir).expect("move dir");
    let victims: Vec<PathBuf> = list_items(&mut client, &lib)
        .iter()
        .filter(|item| {
            item.kind == "static" && item.path.to_string_lossy().contains("/anime/seasonal/")
        })
        .take(20)
        .map(|item| item.path.clone())
        .collect();
    checks.check("move candidates found", victims.len() == 20, || format!("{}", victims.len()));
    for victim in &victims {
        fs::rename(victim, move_dir.join(victim.file_name().unwrap())).expect("rename victim");
    }
    let start = Instant::now();
    let mut moved_now = 0;
    while start.elapsed() < Duration::from_secs(120) {
        moved_now = list_items(&mut client, &lib)
            .iter()
            .filter(|item| item.path.to_string_lossy().contains("/moved/into/here/"))
            .count();
        if moved_now == 20 {
            break;
        }
        std::thread::sleep(Duration::from_millis(400));
    }
    checks.check("moved files re-imported at new path", moved_now == 20, || {
        format!("{moved_now}/20 after {:.1}s", start.elapsed().as_secs_f64())
    });
    let stale =
        list_items(&mut client, &lib).iter().filter(|item| victims.contains(&item.path)).count();
    checks.check("old paths dropped after move", stale == 0, || format!("{stale} stale"));
    let got = list_count(&mut client, &lib);
    checks.check("no items lost after move", got == total, || format!("{got}/{total}"));
    total = got;

    let statics_now: Vec<Item> =
        list_items(&mut client, &lib).into_iter().filter(|item| item.kind == "static").collect();
    let one = statics_now.first().expect("a static to delete");
    let thumb = one.thumb.clone();
    let start = Instant::now();
    fs::remove_file(&one.path).expect("delete static");
    let (dt, _, got) = wait_count(&mut client, &lib, total - 1, walld_pid, Duration::from_secs(30));
    total -= 1;
    checks.check("single delete removes item", got == total, || format!("{dt:.2}s"));
    checks
        .check("single delete settles on debounce not max-hold", dt < 5.0, || format!("{dt:.2}s"));
    let _ = start;
    if !thumb.is_empty() {
        checks.check("thumb cleaned up", !Path::new(&thumb).exists(), || thumb.clone());
    }

    if have_videos {
        fs::remove_file(&vid_paths[0]).expect("delete video");
        let (dt, _, got) =
            wait_count(&mut client, &lib, total - 1, walld_pid, Duration::from_secs(30));
        total -= 1;
        checks.check("video delete removes item", got == total, || format!("{dt:.2}s"));
    }

    let tree = lib.join("deep");
    let tree_items =
        list_items(&mut client, &lib).iter().filter(|item| item.path.starts_with(&tree)).count();
    fs::remove_dir_all(&tree).expect("rmtree deep");
    let (dt, _, got) =
        wait_count(&mut client, &lib, total - tree_items, walld_pid, Duration::from_secs(120));
    total -= tree_items;
    checks.check(&format!("subtree delete removes {tree_items} items"), got == total, || {
        format!("{dt:.1}s")
    });
    checks.check("db consistent after deletes", db_count(&sqlite) == total as i64, || {
        format!("db={} list={total}", db_count(&sqlite))
    });

    checks.check("no panics in walld log", !walld.log_contents().contains("panicked"), String::new);
    let end_pss = pss_mb(walld_pid);
    checks.check("walld PSS stays bounded", end_pss < 200.0, || format!("{end_pss:.1} MB"));
    eprintln!("  info  end: {total} items, walld {end_pss:.1} MB PSS");

    if checks.failed() {
        sandbox.mark_failed();
    }
    checks.finish();
}
