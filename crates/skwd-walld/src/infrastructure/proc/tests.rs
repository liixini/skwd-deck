#![cfg(test)]

use std::fs;
use std::path::Path;

const RENDERERS_OUTLIVE_WALLD: [&str; 1] = ["overview_backdrop.rs"];

#[test]
fn tools_use_proc_tool() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    let mut stack = vec![src.clone()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read src") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            if path.extension().is_none_or(|ext| ext != "rs")
                || name == "tests.rs"
                || name == "proc.rs"
                || RENDERERS_OUTLIVE_WALLD.contains(&name.as_str())
            {
                continue;
            }
            let rel = path.strip_prefix(&src).unwrap_or(&path).display().to_string();
            let text = fs::read_to_string(&path).unwrap_or_default();
            for (idx, line) in text.lines().enumerate() {
                if line.contains("Command::new(") {
                    offenders.push(format!("{rel}:{}", idx + 1));
                }
            }
        }
    }
    assert!(offenders.is_empty(), "use proc::tool(): {offenders:?}");
}
