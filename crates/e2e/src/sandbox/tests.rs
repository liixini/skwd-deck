use super::{CPU_SCANNER, LEGACY_RENDERERS, MISSING_PAPER, Sandbox};
use serde_json::{Value, json};

fn written_config(sandbox: &Sandbox) -> Value {
    serde_json::from_slice(&std::fs::read(sandbox.config_path()).expect("config file"))
        .expect("config json")
}

#[test]
fn legacy_renderer_disables_paper() {
    let mut sandbox = Sandbox::new("legacy-renderer-config");
    sandbox.set_env(LEGACY_RENDERERS[0], "/test/stub");
    sandbox.write_config(&json!({"paths": {"wallpaper": sandbox.library()}}));
    assert_eq!(
        written_config(&sandbox)["paths"]["paperBin"],
        json!(sandbox.root.join(MISSING_PAPER))
    );
}

#[test]
fn explicit_paper_is_preserved() {
    let mut sandbox = Sandbox::new("explicit-paper-config");
    sandbox.set_env(LEGACY_RENDERERS[1], "/test/stub");
    let config = json!({"paths": {"paperBin": "/test/paper"}});
    sandbox.write_config(&config);
    assert_eq!(written_config(&sandbox), config);
}

#[test]
fn isolates_ipc() {
    let sandbox = Sandbox::new("runtime-env");
    let environment = sandbox.env();
    let paper = sandbox.root.join("runtime/paper.sock").to_string_lossy().into_owned();
    let plasma = sandbox.root.join("runtime/skwd-paper-plasma").to_string_lossy().into_owned();
    assert!(!environment.iter().any(|(key, _)| key == "XDG_RUNTIME_DIR"));
    assert!(
        environment.iter().any(|(key, value)| { key == CPU_SCANNER.0 && value == CPU_SCANNER.1 })
    );
    assert!(
        environment.iter().any(|(key, path)| { key == "SKWD_PAPER_V2_SOCKET" && path == &paper })
    );
    assert!(environment.iter().any(|(key, path)| {
        key == skwd_wall_core::plasma::PRESENTATION_DIRECTORY_ENV && path == &plasma
    }));
}
