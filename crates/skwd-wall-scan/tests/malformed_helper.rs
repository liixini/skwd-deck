#![cfg(target_os = "linux")]

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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
    let config = root.join("config/skwd-wall-v2");
    std::fs::create_dir_all(&wallpaper).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(root.join("runtime")).unwrap();
    write_bmp(&wallpaper.join("ok.bmp"));
    std::fs::write(
        config.join("config.json"),
        format!(r#"{{"paths":{{"wallpaper":"{}"}}}}"#, wallpaper.display()),
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

fn run_helper(root: &Path, arguments: &[&str], socket: Option<&Path>) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_skwd-wall-scan"));
    command
        .args(arguments)
        .env("HOME", root)
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_RUNTIME_DIR", root.join("runtime"))
        .env("SKWD_WALL_V2_CONFIG", root.join("config/skwd-wall-v2"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(socket) = socket {
        command.env("SKWD_WALL_V2_SOCK", socket);
    }
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
