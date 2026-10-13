use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};

const ACTIVITIES_FILE: &str = "activities.json";
const ACTIVITY_UNAVAILABLE_FILE: &str = "activity-unavailable";
const ASSIGNMENTS_UNAVAILABLE_FILE: &str = "assignments-unavailable";
const NATIVE_OUTPUTS_FILE: &str = "native-outputs.json";
const ASSIGNMENTS_FILE: &str = "assignments.json";
const DEFAULT_ACTIVITIES: &str =
    r#"{"current":"activity-a","names":{"activity-a":"Work","activity-b":"Home"}}"#;
const CURRENT_METHOD: &str = ".CurrentActivity";
const LIST_METHOD: &str = ".ListActivities";
const NAME_METHOD: &str = ".ActivityName";
const SCRIPT_TARGET: &str = "var target=";
const FORGET_ID: &str = "var id=";
const FORGET_FOLDER: &str = "var folder=";
const FORGET_ACTUAL: &str = "var actual=";
const WALLPAPER: &str = "wallpaper";
const METHOD_ARG: usize = 2;
const VALUE_ARG: usize = 3;
const SCRIPT_VISIBLE: &str = "!==true";

#[derive(serde::Deserialize)]
struct Activities {
    current: String,
    names: std::collections::BTreeMap<String, String>,
}

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

fn fixture(dir: &Path, name: &str, fallback: Value) -> Value {
    std::fs::read(dir.join(name))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(fallback)
}

fn forget_params(script: &str) -> Option<(String, String, String)> {
    let value = |prefix| {
        script
            .split(prefix)
            .nth(1)
            .and_then(|text| text.split(';').next())
            .and_then(|text| serde_json::from_str::<String>(text).ok())
    };
    Some((value(FORGET_ID)?, value(FORGET_FOLDER)?, value(FORGET_ACTUAL)?))
}

fn in_folder(path: &str, folder: &str) -> bool {
    path == folder || path.strip_prefix(folder).is_some_and(|suffix| suffix.starts_with('/'))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(dir) = std::env::var_os("SKWD_E2E_PLASMA").map(PathBuf::from) else {
        std::process::exit(1);
    };
    record(&dir, &args);
    let Some(method) = args.get(METHOD_ARG) else { return };
    if method.ends_with(CURRENT_METHOD) && dir.join(ACTIVITY_UNAVAILABLE_FILE).exists() {
        std::process::exit(1);
    }
    let activities: Activities = serde_json::from_value(fixture(
        &dir,
        ACTIVITIES_FILE,
        serde_json::from_str(DEFAULT_ACTIVITIES).unwrap(),
    ))
    .unwrap();
    if method.ends_with(CURRENT_METHOD) {
        println!("{}", activities.current);
        return;
    }
    if method.ends_with(LIST_METHOD) {
        for id in activities.names.keys() {
            println!("{id}");
        }
        return;
    }
    if method.ends_with(NAME_METHOD) {
        println!("{}", activities.names[&args[VALUE_ARG]]);
        return;
    }
    if method != skwd_e2e::EVALUATE_SCRIPT {
        return;
    }
    let script = &args[VALUE_ARG];
    let mut saved = fixture(&dir, ASSIGNMENTS_FILE, json!({}));
    if let Some((id, folder, actual)) = forget_params(script) {
        if let Some(activities) = saved.as_object_mut() {
            for entries in activities.values_mut().filter_map(Value::as_object_mut) {
                entries.retain(|_, entry| {
                    let item = entry.get(WALLPAPER).unwrap_or(entry);
                    let path = item[wall_proto::apply_fields::PATH].as_str().unwrap_or_default();
                    item[wall_proto::apply_fields::WE_ID] != id
                        && !in_folder(path, &folder)
                        && !in_folder(path, &actual)
                });
            }
        }
        std::fs::write(dir.join(ASSIGNMENTS_FILE), saved.to_string()).unwrap();
        return;
    }
    let Some(target) = script
        .split(SCRIPT_TARGET)
        .nth(1)
        .and_then(|text| text.split(';').next())
        .and_then(|text| serde_json::from_str::<String>(text).ok())
    else {
        return;
    };
    if !activities.names.contains_key(&target) {
        std::process::exit(1);
    }
    let Some(assignments) = skwd_e2e::script_assignments(script) else {
        if dir.join(ASSIGNMENTS_UNAVAILABLE_FILE).exists() {
            println!("{{}}");
            return;
        }
        let mut entries = saved.get(&target).cloned().unwrap_or(json!({}));
        let native = fixture(&dir, NATIVE_OUTPUTS_FILE, json!({}));
        if let Some(outputs) = native[&target].as_array() {
            for output in outputs.iter().filter_map(Value::as_str) {
                entries.as_object_mut().unwrap().remove(output);
            }
        }
        println!("{entries}");
        return;
    };
    let current = activities.current == target;
    if script.contains(SCRIPT_VISIBLE) != current {
        std::process::exit(1);
    }
    if saved.get(&target).is_none() {
        saved[&target] = json!({});
    }
    for (output, entry) in &assignments {
        saved[&target][output] = entry.clone();
    }
    std::fs::write(dir.join(ASSIGNMENTS_FILE), saved.to_string()).unwrap();
    if !current {
        return;
    }
    let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") else {
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
