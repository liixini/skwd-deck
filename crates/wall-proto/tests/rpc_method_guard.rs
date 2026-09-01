use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use wall_proto::rpc;

const METHOD_CALLS: &[&str] = &["call", "send", "remap"];
const CONFIG_KEY_PATH: &str = "skwd_config::keys::";
const CATALOG: &str = "wall-proto/src/rpc_catalog.rs";
const SKIPPED_CRATES: &[&str] = &["skwd-config", "e2e"];

#[test]
fn method_literals_in_catalog() {
    let names: BTreeSet<&str> =
        rpc::ALL.iter().copied().filter(|name| name.contains('.')).collect();
    let mut offenders = Vec::new();
    for (relative, source) in production_sources() {
        if relative == CATALOG {
            continue;
        }
        for (line, literal) in string_literals(&source) {
            if names.contains(literal.as_str()) {
                offenders.push(format!("{relative}:{line} \"{literal}\""));
            }
        }
    }
    assert!(offenders.is_empty(), "method literals:\n{}", offenders.join("\n"));
}

#[test]
fn method_arguments_catalog_paths() {
    let mut offenders = Vec::new();
    for (relative, source) in production_sources() {
        for line in bad_method_argument_lines(&source) {
            offenders.push(format!("{relative}:{line}"));
        }
    }
    assert!(offenders.is_empty(), "non-catalog method arguments:\n{}", offenders.join("\n"));
}

#[test]
fn catalog_methods_dispatched() {
    let router = fs::read_to_string(
        workspace_root().join("crates/skwd-walld/src/infrastructure/rpc/router.rs"),
    )
    .expect("read router.rs");
    let connection = fs::read_to_string(
        workspace_root().join("crates/skwd-walld/src/infrastructure/rpc/connection.rs"),
    )
    .expect("read connection.rs");
    let dispatched = format!("{router}{connection}");
    let mut missing = Vec::new();
    for (name, value) in catalog_consts() {
        if !dispatched.contains(&format!("rpc::{name}")) {
            missing.push(format!("{name} (\"{value}\")"));
        }
    }
    assert_eq!(missing, vec!["PREVIEW_READY (\"preview.ready\")".to_string()]);
}

#[test]
fn scanner_flags_offenders() {
    let wrapped = concat!(
        "call(\n",
        "    socket,\n",
        "    skwd_config::keys::theme::BACKEND,\n",
        "    &json!({}),\n",
        ");\n",
    );
    assert_eq!(bad_method_argument_lines(wrapped), [1]);
    let inline = "reporter.send(skwd_config::keys::theme::BACKEND, &json!({}));\n";
    assert_eq!(bad_method_argument_lines(inline), [1]);
    let literal = "connection.call(\"diag\", &serde_json::json!({}));\n";
    assert_eq!(bad_method_argument_lines(literal), [1]);
}

#[test]
fn scanner_allows_catalog_paths() {
    let allowed = concat!(
        "call(socket, rpc::WALL_RETHEME, &json!({}))?;\n",
        "reporter.send(rpc::SCAN_ITEM, item);\n",
        "call(socket, method, &json!({ \"output\": output }))?;\n",
        "let value = config.str_at(skwd_config::keys::theme::BACKEND, \"\");\n",
        "call(socket, rpc::WALL_APPLY, &json!({\"k\": skwd_config::keys::theme::BACKEND}))?;\n",
        "dispatch(ctx, &remap(req, &native));\n",
    );
    assert!(bad_method_argument_lines(allowed).is_empty());
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().to_path_buf()
}

fn production_sources() -> Vec<(String, String)> {
    let root = workspace_root();
    let crates = root.join("crates");
    let mut sources = Vec::new();
    for entry in fs::read_dir(&crates).expect("read crates/") {
        let path = entry.expect("directory entry").path();
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        if !path.is_dir() || SKIPPED_CRATES.contains(&name.as_str()) {
            continue;
        }
        let mut files = Vec::new();
        rust_files(&path.join("src"), &mut files);
        for file in files {
            let relative = file.strip_prefix(&crates).unwrap_or(&file).display().to_string();
            let source = fs::read_to_string(&file)
                .unwrap_or_else(|error| panic!("read {relative}: {error}"));
            sources.push((relative, source));
        }
    }
    assert!(!sources.is_empty());
    sources
}

fn rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    if !directory.is_dir() {
        return;
    }
    for entry in fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
    {
        let path = entry.expect("directory entry").path();
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        if path.is_dir() {
            if name != "tests" {
                rust_files(&path, files);
            }
        } else if name.ends_with(".rs") && name != "tests.rs" && !name.ends_with("_tests.rs") {
            files.push(path);
        }
    }
}

fn catalog_consts() -> Vec<(String, String)> {
    let source =
        fs::read_to_string(workspace_root().join("crates").join(CATALOG)).expect("read catalog");
    source
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("pub const ")?;
            let (name, rest) = rest.split_once(": &str = \"")?;
            Some((name.to_string(), rest.strip_suffix("\";")?.to_string()))
        })
        .collect()
}

fn string_literals(source: &str) -> Vec<(usize, String)> {
    let bytes = source.as_bytes();
    let mut literals = Vec::new();
    let mut line = 1usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => line += 1,
            b'"' => {
                let start = index + 1;
                index = start;
                while index < bytes.len() && bytes[index] != b'"' {
                    index += usize::from(bytes[index] == b'\\') + 1;
                }
                literals.push((line, String::from_utf8_lossy(&bytes[start..index]).into_owned()));
            }
            _ => {}
        }
        index += 1;
    }
    literals
}

fn bad_method_argument_lines(source: &str) -> Vec<usize> {
    let bytes = source.as_bytes();
    let mut offenders: Vec<usize> = Vec::new();
    let mut line = 1usize;
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'\n' {
            line += 1;
            index += 1;
            continue;
        }
        let Some(after_paren) = call_argument_start(bytes, index) else {
            index += 1;
            continue;
        };
        let mut cursor = after_paren;
        for _ in 0..2 {
            let (argument, next) = argument_at(bytes, cursor);
            if is_method_offender(&argument) && offenders.last() != Some(&line) {
                offenders.push(line);
            }
            let Some(next) = next else { break };
            cursor = next;
        }
        index = after_paren;
    }
    offenders
}

fn is_method_offender(argument: &str) -> bool {
    let trimmed = argument.trim().trim_start_matches('&');
    if trimmed.starts_with(CONFIG_KEY_PATH) {
        return true;
    }
    let Some(inner) = trimmed.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')) else {
        return false;
    };
    inner.chars().all(|character| character.is_ascii_lowercase() || character == '.')
        && !inner.is_empty()
}

fn call_argument_start(bytes: &[u8], index: usize) -> Option<usize> {
    if index > 0 && is_ident(bytes[index - 1]) {
        return None;
    }
    METHOD_CALLS.iter().find_map(|name| {
        let end = index + name.len();
        (bytes.get(index..end) == Some(name.as_bytes()) && bytes.get(end) == Some(&b'('))
            .then_some(end + 1)
    })
}

fn argument_at(bytes: &[u8], start: usize) -> (String, Option<usize>) {
    let mut depth = 0usize;
    let mut index = start;
    let mut next = None;
    while index < bytes.len() {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            b',' if depth == 0 => {
                next = Some(index + 1);
                break;
            }
            b'"' => {
                index += 1;
                while index < bytes.len() && bytes[index] != b'"' {
                    index += usize::from(bytes[index] == b'\\') + 1;
                }
            }
            _ => {}
        }
        index += 1;
    }
    (String::from_utf8_lossy(&bytes[start..index.min(bytes.len())]).into_owned(), next)
}

fn is_ident(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric()
}
