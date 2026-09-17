use serde_json::json;
use std::io::Write;
use std::path::{Path, PathBuf};

fn record(dir: &Path, args: &[String]) {
    if let Ok(mut log) =
        std::fs::OpenOptions::new().create(true).append(true).open(dir.join("qdbus.log"))
    {
        let _ = writeln!(log, "{}", json!(args));
    }
}

fn presentation(dir: &Path) -> serde_json::Value {
    std::fs::read_to_string(dir.join("failure")).map_or_else(
        |_| json!({"state": "ready"}),
        |error| json!({"state": "error", "error": error}),
    )
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(dir) = std::env::var_os("SKWD_E2E_PLASMA").map(PathBuf::from) else {
        std::process::exit(1);
    };
    record(&dir, &args);
    let [_, _, method, script] = args.as_slice() else { return };
    if method != skwd_e2e::EVALUATE_SCRIPT {
        return;
    }
    let (Some(assignments), Some(runtime)) =
        (skwd_e2e::script_assignments(script), std::env::var_os("XDG_RUNTIME_DIR"))
    else {
        return;
    };
    let status = presentation(&dir).to_string();
    let root = Path::new(&runtime).join("skwd-paper-plasma");
    for entry in assignments.values() {
        if let Some(id) = entry["presentationId"].as_str() {
            let _ = std::fs::write(root.join(format!("{id}.json")), &status);
        }
    }
}
