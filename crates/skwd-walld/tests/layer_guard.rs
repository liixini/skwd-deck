use std::path::{Path, PathBuf};

fn production_rust_files(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(path).expect("read source layer") {
            let path = entry.expect("read source entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs")
                && path.file_name().is_none_or(|name| name != "tests.rs")
            {
                files.push(path);
            }
        }
    }
    files
}

fn assert_layer_excludes(root: &Path, forbidden: &[&str]) {
    for path in production_rust_files(root) {
        let source = std::fs::read_to_string(&path).expect("read layer source");
        for dependency in forbidden {
            assert!(!source.contains(dependency), "{} uses `{dependency}`", path.display());
        }
    }
}

#[test]
fn domain_layer_isolated() {
    let domain = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/domain");
    let forbidden = [
        "crate::backend",
        "crate::infrastructure",
        "serde_json",
        "wall_proto",
        "skwd_wall_core",
        "std::fs",
        "std::process",
        "std::net",
        "ureq",
        "notify",
    ];

    assert_layer_excludes(&domain, &forbidden);
}

#[test]
fn backend_ports_isolated() {
    let backend = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend");
    let forbidden = [
        "crate::infrastructure",
        "skwd_wall_core",
        "std::fs",
        "std::process",
        "std::net",
        "ureq",
        "notify",
        "libc::",
    ];
    assert_layer_excludes(&backend, &forbidden);
}

#[test]
fn context_named_services() {
    let context = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/composition/context.rs");
    let source = std::fs::read_to_string(context).expect("read composition context");

    assert!(source.contains("EventHub"));
    assert!(source.contains("MediaWorkerSupervisor"));
    assert!(source.contains("WallpaperApplication"));
    assert!(source.contains("HistoryRepository"));
    assert!(source.contains("ConfigStore"));
    assert!(source.contains("Database"));
    assert!(source.contains("RendererSupervision"));
    assert!(!source.contains("infrastructure::renderers::RendererSupervisor"));
    for raw_state in ["SubList", "SyncSender", "Mutex<", "Vec<"] {
        assert!(!source.contains(raw_state), "leaked `{raw_state}`");
    }
}

#[test]
fn history_port_split() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repository = manifest.join("src/backend/history/repository.rs");
    let file_adapter = manifest.join("src/infrastructure/history/file_repository.rs");

    let port = std::fs::read_to_string(repository).expect("read history port");
    let adapter = std::fs::read_to_string(file_adapter).expect("read history adapter");
    assert!(port.contains("trait HistoryRepository"));
    for implementation_detail in ["std::fs", "PathBuf", "Mutex<", "history.json"] {
        assert!(
            !port.contains(implementation_detail),
            "history port leaked `{implementation_detail}`"
        );
    }
    assert!(adapter.contains("impl HistoryRepository for FileHistoryRepository"));
}

#[test]
fn thin_composition_root() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let main =
        std::fs::read_to_string(manifest.join("src/main.rs")).expect("read daemon entrypoint");
    assert!(main.lines().count() <= 120, "main.rs {} lines", main.lines().count());
    for module in ["backend", "composition", "domain", "infrastructure"] {
        assert!(main.contains(&format!("mod {module};")), "missing `{module}`");
    }
    for removed in [
        "mod apply_runtime;",
        "mod bootstrap;",
        "mod ctx;",
        "mod dispatch;",
        "mod history_runtime;",
        "mod media_paths;",
        "mod persistence;",
        "mod platform;",
        "mod workspaces;",
    ] {
        assert!(!main.contains(removed), "restored `{removed}`");
    }
    for implementation in [
        "fn apply_core(",
        "fn history_nav(",
        "fn persist_last(",
        "fn await_converted_by(",
        "fn spawn_heartbeat(",
        "fn secure_socket_dir(",
        "thread::spawn",
        "UnixListener",
        "serde_json::json",
        "std::fs",
    ] {
        assert!(!main.contains(implementation), "main.rs owns `{implementation}`");
    }

    for implementation in [
        "src/composition/apply.rs",
        "src/composition/bootstrap.rs",
        "src/composition/context.rs",
        "src/composition/history.rs",
        "src/infrastructure/media_paths.rs",
        "src/infrastructure/persistence.rs",
        "src/infrastructure/platform.rs",
    ] {
        assert!(manifest.join(implementation).is_file(), "no owner for `{implementation}`");
    }
}

#[test]
fn source_root_contents() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut root_files: Vec<String> = std::fs::read_dir(&source_root)
        .expect("read daemon source root")
        .filter_map(|entry| {
            let path = entry.expect("read daemon source entry").path();
            path.is_file().then(|| path.file_name()?.to_str().map(str::to_string)).flatten()
        })
        .collect();
    root_files.sort();
    assert_eq!(root_files, ["main.rs", "testenv.rs", "tests.rs"]);

    for removed in ["dispatch.rs", "ctx.rs", "workspaces.rs", "apply_runtime.rs"] {
        assert!(!source_root.join(removed).exists(), "src/{removed} returned");
    }
}

#[test]
fn module_maps_thin() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let maps = [
        (
            "src/infrastructure/rpc/mod.rs",
            &[
                "connection",
                "handlers",
                "response",
                "router",
                "source",
                "source_steam",
                "source_wallhaven",
                "source_youtube",
                "wallpaper",
            ][..],
        ),
        (
            "src/infrastructure/workspaces/mod.rs",
            &["hypr", "kwin", "model", "niri", "policy", "provider", "runtime", "storage"][..],
        ),
        ("src/composition/runtime/mod.rs", &["playlist", "rotation", "schedule"][..]),
    ];
    for (relative, modules) in maps {
        let source =
            std::fs::read_to_string(manifest.join(relative)).expect("read daemon module map");
        assert!(source.lines().count() <= 24, "{relative} too long");
        for module in modules {
            assert!(source.contains(&format!("mod {module};")), "{relative} missing `{module}`");
        }
        for implementation in
            ["fn ", "pub fn ", "pub(crate) fn ", "struct ", "pub struct ", "enum ", "impl "]
        {
            assert!(
                !source.lines().any(|line| line.trim_start().starts_with(implementation)),
                "{relative} has `{implementation}`"
            );
        }
    }

    for (relative, maximum) in [
        ("src/infrastructure/rpc/router.rs", 300),
        ("src/infrastructure/rpc/source.rs", 450),
        ("src/infrastructure/workspaces/runtime.rs", 400),
    ] {
        let source =
            std::fs::read_to_string(manifest.join(relative)).expect("read daemon implementation");
        assert!(source.lines().count() <= maximum, "{relative} {} lines", source.lines().count());
    }
}

#[test]
fn workflows_not_infrastructure() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let infrastructure = std::fs::read_to_string(manifest.join("src/infrastructure/mod.rs"))
        .expect("read infrastructure map");
    let runtime = std::fs::read_to_string(manifest.join("src/composition/runtime/mod.rs"))
        .expect("read composition runtime map");

    for workflow in ["playlist", "rotation", "schedule"] {
        assert!(
            !infrastructure.contains(&format!("mod {workflow};")),
            "infrastructure claims `{workflow}`"
        );
        assert!(runtime.contains(&format!("mod {workflow};")), "runtime missing `{workflow}`");
        assert!(
            manifest.join(format!("src/composition/runtime/{workflow}.rs")).is_file(),
            "no impl for `{workflow}`"
        );
    }
}

#[test]
fn canonical_layer_paths() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let forbidden = [
        "crate::ctx",
        "crate::dispatch",
        "crate::apply_runtime",
        "crate::history_runtime",
        "crate::media_paths",
        "crate::persistence",
        "crate::platform",
        "crate::playlist",
        "crate::rotation",
        "crate::schedule",
        "crate::workspaces",
    ];
    for path in production_rust_files(&source_root) {
        let source = std::fs::read_to_string(&path).expect("read production source");
        for legacy_path in forbidden {
            assert!(!source.contains(legacy_path), "{} imports `{legacy_path}`", path.display());
        }
    }
}

#[test]
fn apply_publication_requires_a_committed_receipt() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let orchestration = std::fs::read_to_string(manifest.join("src/composition/apply.rs"))
        .expect("read apply orchestration");
    let phases = std::fs::read_to_string(manifest.join("src/composition/apply/phases.rs"))
        .expect("read apply phases");

    for boundary in [
        "struct ApplyDecision",
        "struct ExecutionReceipt",
        "struct CommittedApply",
        "fn commit(",
        "fn publish(",
    ] {
        assert!(phases.contains(boundary), "missing typed apply boundary `{boundary}`");
    }
    assert!(
        phases.contains("authorize_commit(self.decision.generation"),
        "renderer handoff lacks a supersession check"
    );
    for leaked_publication in [
        "persist_last(",
        "record_history(",
        "theme().set_source",
        "stats.applied(",
        ".publish(ev::APPLIED",
    ] {
        assert!(
            !orchestration.contains(leaked_publication),
            "execution orchestration reclaimed post-commit effect `{leaked_publication}`"
        );
        assert!(
            phases.contains(leaked_publication),
            "post-commit owner lost `{leaked_publication}`"
        );
    }
}
