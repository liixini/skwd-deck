#![cfg(target_os = "linux")]

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TEST_ROOT_ENV: &str = "SKWD_SCAN_TEST_ROOT";
const SOCKET_ROOT_ENV: &str = "SKWD_SCAN_TEST_SOCKET_ROOT";
const IMAGE_NAMES: [&str; 3] = ["a.bmp", "b.bmp", "c.bmp"];
const ARTIFACTS: [(&str, &str); 4] =
    [("thumbs", "webp"), ("thumbs-sm", "webp"), ("blocks", "bc7"), ("blocks", "bc1")];
const GPU_FAILURE: &str = "gpu thumbnail encoding failed";
const GPU_READY: &str = "gpu encoder ready";
const SCAN_DONE: &str = "scan.done";
const SCAN_ITEM: &str = "scan.item";

#[test]
fn malformed_image_helpers_exit_without_surviving_children() {
    let root = std::env::temp_dir().join(format!("skwd-malformed-helper-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let malformed = root.join("broken.png");
    std::fs::write(&malformed, b"\x89PNG\r\n\x1a\ntruncated").unwrap();

    run_helper(&root, &["--tone", malformed.to_str().unwrap()], None);
    run_helper(&root, &["--preview", "video:broken.mp4", malformed.to_str().unwrap()], None);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn sandboxed_full_scan_reads_library_and_writes_deck_state() {
    let root = std::env::temp_dir().join(format!("skwd-sandboxed-scan-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let wallpaper = root.join("wallpaper");
    let workshop = root.join("workshop");
    let config = root.join("config/skwd-wall-v2");
    std::fs::create_dir_all(&wallpaper).unwrap();
    std::fs::create_dir_all(&workshop).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(root.join("runtime")).unwrap();
    write_bmp(&wallpaper.join("ok.bmp"));
    std::fs::write(
        config.join("config.json"),
        format!(
            r#"{{"paths":{{"wallpaper":"{}","steamWorkshop":"{}"}}}}"#,
            wallpaper.display(),
            workshop.display()
        ),
    )
    .unwrap();

    let socket = root.join("runtime/reporter.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut reporter, _) = listener.accept().unwrap();
        let mut messages = String::new();
        reporter.read_to_string(&mut messages).unwrap();
        reporter.write_all(b"{\"ok\":true,\"id\":0}\n").unwrap();
        messages
    });
    run_helper(&root, &[], Some(&socket));
    let messages = server.join().unwrap();

    assert!(root.join("cache/skwd-wall-v2/thumbs/ok.webp").is_file());
    assert!(root.join("data/skwd-wall-v2/wall.sqlite").is_file());
    assert!(messages.lines().any(|line| line.contains(r#""method":"scan.done""#)));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn fallback_completes_cache() {
    let root = test_root(TEST_ROOT_ENV);
    let cpu = tempfile::Builder::new().prefix("cpu-").tempdir_in(&root).unwrap();
    let gpu = tempfile::Builder::new().prefix("gpu-").tempdir_in(&root).unwrap();
    for root in [cpu.path(), gpu.path()] {
        let wallpaper = root.join("wallpaper");
        let workshop = root.join("workshop");
        let config = root.join("config/skwd-wall-v2");
        std::fs::create_dir_all(&wallpaper).unwrap();
        std::fs::create_dir_all(&workshop).unwrap();
        std::fs::create_dir_all(&config).unwrap();
        for name in IMAGE_NAMES {
            write_bmp(&wallpaper.join(name));
        }
        std::fs::write(
            config.join("config.json"),
            format!(
                r#"{{"paths":{{"wallpaper":"{}","steamWorkshop":"{}"}}}}"#,
                wallpaper.display(),
                workshop.display()
            ),
        )
        .unwrap();
    }

    let (cpu_log, cpu_report) = run_scan(cpu.path(), false);
    assert!(!cpu_log.contains(GPU_FAILURE));
    assert_scan(&cpu_report, cpu.path(), IMAGE_NAMES.len());
    let reference = artifacts(cpu.path());

    let (gpu_log, gpu_report) = run_scan(gpu.path(), true);
    assert_eq!(gpu_log.matches(GPU_FAILURE).count(), 1);
    assert_scan(&gpu_report, gpu.path(), IMAGE_NAMES.len());
    assert_artifacts(gpu.path(), &reference);

    let before = artifacts(gpu.path());
    let (rescan_log, rescan_report) = run_scan(gpu.path(), true);
    assert_scan(&rescan_report, gpu.path(), 0);
    assert!(!rescan_log.contains(GPU_FAILURE));
    assert!(!rescan_log.contains(GPU_READY));
    assert_artifacts(gpu.path(), &before);
}

#[test]
fn idle_gpu_worker_exits() {
    let root =
        tempfile::Builder::new().prefix("worker-").tempdir_in(test_root(TEST_ROOT_ENV)).unwrap();
    let cache = root.path().join("shaders");
    std::fs::create_dir_all(&cache).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_skwd-wall-scan"));
    command
        .arg("--gpu-thumbnail-worker")
        .arg(&cache)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let output = wait_output(command, Duration::from_secs(5));
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(GPU_READY));
}

fn run_scan(root: &Path, gpu: bool) -> (String, Vec<serde_json::Value>) {
    let socket_dir =
        tempfile::Builder::new().prefix("s-").tempdir_in(test_root(SOCKET_ROOT_ENV)).unwrap();
    let socket = socket_dir.path().join("r");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut reporter, _) = listener.accept().unwrap();
        let mut messages = String::new();
        reporter.read_to_string(&mut messages).unwrap();
        reporter.write_all(b"{\"ok\":true,\"id\":0}\n").unwrap();
        messages.lines().map(|line| serde_json::from_str(line).unwrap()).collect::<Vec<_>>()
    });
    let mut command = helper_command(root, &socket);
    let log = root.join("scanner.log");
    command
        .env("SKWD_SCAN_GPU", if gpu { "1" } else { "0" })
        .env("SKWD_SCAN_THREADS", "1")
        .env("SKWD_WALL_LOG", "info")
        .env("VK_DRIVER_FILES", root.join("missing-icd.json"))
        .env("VK_ICD_FILENAMES", root.join("missing-icd.json"))
        .stdout(Stdio::null())
        .stderr(Stdio::from(std::fs::File::create(&log).unwrap()));
    let output = wait_output(command, Duration::from_secs(45));
    let log = std::fs::read_to_string(log).unwrap();
    assert!(output.status.success(), "{log}");
    let report = server.join().unwrap();
    (log, report)
}

fn assert_scan(messages: &[serde_json::Value], root: &Path, expected: usize) {
    let items = messages.iter().filter(|message| message["method"] == SCAN_ITEM).count();
    let done: Vec<_> = messages.iter().filter(|message| message["method"] == SCAN_DONE).collect();
    assert_eq!(items, expected);
    assert_eq!(done.len(), 1);
    assert_eq!(done[0]["params"]["count"], expected);
    let database = root.join("data/skwd-wall-v2/wall.sqlite");
    let connection = rusqlite::Connection::open(database).unwrap();
    let rows = connection
        .query_row("SELECT COUNT(*) FROM meta", [], |row| row.get::<_, usize>(0))
        .unwrap();
    assert_eq!(rows, IMAGE_NAMES.len());
}

fn artifacts(root: &Path) -> Vec<Vec<u8>> {
    IMAGE_NAMES
        .iter()
        .flat_map(|name| {
            let stem = name.strip_suffix(".bmp").unwrap();
            ARTIFACTS.iter().map(move |(group, ext)| {
                let path =
                    root.join("cache/skwd-wall-v2").join(group).join(format!("{stem}.{ext}"));
                std::fs::read(path).unwrap()
            })
        })
        .collect()
}

fn assert_artifacts(root: &Path, expected: &[Vec<u8>]) {
    let actual = artifacts(root);
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!(actual == expected, "artifact {index}");
    }
}

fn wait_output(mut command: Command, timeout: Duration) -> std::process::Output {
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("scanner exceeded {timeout:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn helper_command(root: &Path, socket: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_skwd-wall-scan"));
    command
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("SKWD_WALL_V2_CONFIG", root.join("config/skwd-wall-v2"))
        .env("SKWD_WALL_V2_SOCK", socket)
        .stdin(Stdio::null());
    command
}

fn test_root(name: &str) -> std::path::PathBuf {
    std::env::var_os(name).map_or_else(std::env::temp_dir, std::path::PathBuf::from)
}

fn run_helper(root: &Path, arguments: &[&str], socket: Option<&Path>) {
    let socket = socket.map_or_else(|| root.join("r.sock"), Path::to_path_buf);
    let mut command = helper_command(root, &socket);
    command.args(arguments).env("SKWD_SCAN_GPU", "0").stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = command.spawn().unwrap();
    let pid = child.id();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "helper {arguments:?} exited with {status}");
            assert!(!Path::new(&format!("/proc/{pid}")).exists());
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("helper {arguments:?} exceeded five seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn write_bmp(path: &Path) {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&58u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&1i32.to_le_bytes());
    bytes.extend_from_slice(&1i32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&24u16.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.extend_from_slice(&2835i32.to_le_bytes());
    bytes.extend_from_slice(&2835i32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&[40, 80, 160, 0]);
    std::fs::write(path, bytes).unwrap();
}
