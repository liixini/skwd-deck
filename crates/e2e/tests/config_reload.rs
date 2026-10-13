use std::io::Write;
use std::process::Command;
use std::sync::{Arc, mpsc};
use std::time::{Duration, SystemTime};

use skwd_wall_core::config::ConfigStore;

const CHILD: &str = "SKWD_CONFIG_RELOAD_CHILD";
const WAIT: Duration = Duration::from_secs(2);

#[test]
fn concurrent_reload() {
    if std::env::var_os(CHILD).is_some() {
        reload_files();
        return;
    }
    let mut sandbox = skwd_e2e::Sandbox::new("config-reload");
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "concurrent_reload", "--nocapture"])
        .env(CHILD, "1")
        .env("SKWD_WALL_V2_CONFIG", sandbox.config_path().parent().unwrap())
        .output()
        .unwrap();
    if !output.status.success() {
        sandbox.mark_failed();
    }
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn reload_files() {
    let path = skwd_wall_core::config::config_path();
    std::fs::write(&path, r#"{"library":{"folders":["/initial"]}}"#).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH)
        .unwrap();
    let store = Arc::new(ConfigStore::load());
    std::fs::remove_file(&path).unwrap();
    assert!(Command::new("mkfifo").arg(&path).status().unwrap().success());
    let old_store = store.clone();
    let old = std::thread::spawn(move || old_store.reload_if_changed());
    let mut writer = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::write(&path, r#"{"library":{"folders":["/new"]}}"#).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1))
        .unwrap();
    let new_store = store.clone();
    let (tx, rx) = mpsc::channel();
    let new = std::thread::spawn(move || tx.send(new_store.reload_if_changed()).unwrap());
    let early = rx.recv_timeout(WAIT).ok();
    writer.write_all(br#"{"library":{"folders":["/old"]}}"#).unwrap();
    drop(writer);
    assert!(old.join().unwrap());
    if early.is_none() {
        assert!(rx.recv_timeout(WAIT).unwrap());
    }
    new.join().unwrap();
    assert_eq!(store.read().library_folders(), ["/new"]);
}
