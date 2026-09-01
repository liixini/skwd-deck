use std::path::{Path, PathBuf};

fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).expect("read source directory") {
            let path = entry.expect("read source entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
    files
}

fn production_rust_files(root: &Path) -> Vec<PathBuf> {
    rust_files(root)
        .into_iter()
        .filter(|path| {
            path.file_name().is_none_or(|name| {
                name != "tests.rs" && !name.to_string_lossy().ends_with("_tests.rs")
            })
        })
        .collect()
}

fn assert_layer_excludes(layer: &str, forbidden: &[&str]) {
    let directory = source_root().join(layer);
    for path in production_rust_files(&directory) {
        let source = std::fs::read_to_string(&path).expect("read production source");
        for dependency in forbidden {
            assert!(
                !source.contains(dependency),
                "{} crosses {layer}: {dependency}",
                path.display()
            );
        }
    }
}

#[test]
fn domain_layer_pure() {
    assert_layer_excludes(
        "domain",
        &[
            "crate::backend",
            "crate::composition",
            "crate::infrastructure",
            "serde_json",
            "rusqlite",
            "std::fs",
            "std::process",
            "std::net",
            "wayland",
            "ffmpeg",
            "walkdir",
            "image::",
        ],
    );
}

#[test]
fn backend_ports_only() {
    assert_layer_excludes(
        "backend",
        &[
            "crate::composition",
            "crate::infrastructure",
            "crate::audio",
            "crate::config",
            "crate::db",
            "crate::paths",
            "crate::state",
            "rusqlite",
            "serde_json",
            "std::fs",
            "std::process",
            "wayland",
            "ffmpeg",
        ],
    );
}

#[test]
fn wall_state_composition_root() {
    let state = source_root().join("composition/state/model.rs");
    let source = std::fs::read_to_string(state).expect("read state composition");

    for component in ["ConfigStore", "Database", "RendererSupervisor", "ApplyRuntime"] {
        assert!(source.contains(component), "missing component {component}");
    }
    for primitive in [
        "Mutex<Connection>",
        "Mutex<Vec<Child>>",
        "Mutex<Option<Child>>",
        "Mutex<Option<ChildStdin>>",
        "apply_lock",
        "cfg_mtime",
    ] {
        assert!(!source.contains(primitive), "WallState owns {primitive}");
    }
}

#[test]
fn renderers_own_no_config() {
    assert_layer_excludes(
        "infrastructure/renderers",
        &["ConfigStore", "crate::config", "Database", "rusqlite", "Connection", "history.json"],
    );
}

#[test]
fn crate_root_public_map() {
    let root = source_root();
    let root_files: Vec<_> = std::fs::read_dir(&root)
        .expect("read crate source root")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file() && path.extension().is_some_and(|extension| extension == "rs")
        })
        .collect();
    assert_eq!(root_files, vec![root.join("lib.rs")]);

    let public_map = std::fs::read_to_string(root.join("lib.rs")).expect("read public map");
    for implementation in ["struct ", "enum ", "trait ", "impl ", "fn "] {
        assert!(
            !public_map.lines().any(|line| line.starts_with(implementation)),
            "lib.rs has {implementation}"
        );
    }
}

#[test]
fn module_roots_thin() {
    for path in rust_files(&source_root())
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "mod.rs" || name == "lib.rs"))
    {
        let source = std::fs::read_to_string(&path).expect("read module map");
        for implementation in [
            "pub struct ",
            "pub enum ",
            "pub trait ",
            "pub fn ",
            "pub(crate) fn ",
            "struct ",
            "enum ",
            "trait ",
            "impl ",
            "fn ",
        ] {
            assert!(
                !source.lines().any(|line| line.starts_with(implementation)),
                "{} has {implementation}",
                path.display()
            );
        }
    }
}

#[test]
fn named_owners_exist() {
    let root = source_root();
    for expected in [
        "infrastructure/config/model/source.rs",
        "infrastructure/config/model/value.rs",
        "infrastructure/config/model/value/display.rs",
        "infrastructure/config/model/value/renderer.rs",
        "infrastructure/config/model/value/theme.rs",
        "infrastructure/config/model/value/transition.rs",
        "infrastructure/scan/artifacts.rs",
        "infrastructure/scan/catalog.rs",
        "infrastructure/scan/concurrency.rs",
        "infrastructure/scan/scanner/common.rs",
        "infrastructure/scan/scanner/delta.rs",
        "infrastructure/scan/scanner/images.rs",
        "infrastructure/scan/scanner/status.rs",
        "infrastructure/scan/scanner/videos.rs",
        "infrastructure/scan/scanner/wallpaper_engine.rs",
        "infrastructure/theme/application.rs",
        "infrastructure/theme/availability.rs",
        "infrastructure/media/video/cancellation.rs",
        "infrastructure/media/video/decoding.rs",
        "infrastructure/media/video/extraction.rs",
        "infrastructure/media/video/preview.rs",
        "infrastructure/media/video/preview_policy.rs",
        "infrastructure/wallpaper/apply/engine.rs",
        "infrastructure/wallpaper/apply/lifecycle.rs",
        "infrastructure/media/video/scaling.rs",
        "infrastructure/media/video/source.rs",
        "infrastructure/media/video/switching.rs",
        "infrastructure/wallpaper/apply/orchestrator.rs",
        "infrastructure/wallpaper/apply/policy.rs",
        "infrastructure/wallpaper/apply/reconcile.rs",
        "infrastructure/wallpaper/apply/refresh.rs",
        "infrastructure/wallpaper/apply/resolver.rs",
        "infrastructure/wallpaper/apply/static_media.rs",
        "infrastructure/wallpaper/apply/transition.rs",
        "infrastructure/wallpaper/apply/transaction.rs",
        "infrastructure/wallpaper/apply/video_media.rs",
        "infrastructure/wallpaper/apply/wallpaper_engine.rs",
        "domain/wallpaper/arguments.rs",
    ] {
        assert!(root.join(expected).is_file(), "missing owner {expected}");
    }
    assert!(!root.join("infrastructure/scan/scanner.rs").exists());
    assert!(!root.join("infrastructure/media/video/pipeline.rs").exists());

    for owner in [
        "infrastructure/scan/scanner/common.rs",
        "infrastructure/scan/scanner/delta.rs",
        "infrastructure/scan/scanner/images.rs",
        "infrastructure/scan/scanner/status.rs",
        "infrastructure/scan/scanner/videos.rs",
        "infrastructure/scan/scanner/wallpaper_engine.rs",
    ] {
        let lines =
            std::fs::read_to_string(root.join(owner)).expect("read scan owner").lines().count();
        assert!(lines <= 240, "{owner}: {lines} lines");
    }

    for legacy in [
        "analysis",
        "apply.rs",
        "audio",
        "audio.rs",
        "awww",
        "config.rs",
        "blocks",
        "bridge_preview",
        "config",
        "countalloc",
        "db",
        "diag",
        "dms",
        "material",
        "matugen",
        "media.rs",
        "noctalia",
        "outputs",
        "pack",
        "paths",
        "paths.rs",
        "postprocess",
        "scan",
        "scan.rs",
        "state",
        "state.rs",
        "static_templates",
        "style",
        "theme",
        "theme.rs",
        "theme_sink",
        "we",
    ] {
        assert!(!root.join(legacy).exists(), "legacy src/{legacy}");
    }
}

#[test]
fn config_feature_facades_are_bounded_and_canonical() {
    let root = source_root();
    let config = root.join("infrastructure/config/model");
    let facade = std::fs::read_to_string(config.join("value.rs")).expect("read config facade");

    assert!(facade.contains("pub fn display(&self) -> DisplayConfig<'_>"));
    assert!(facade.contains("pub fn renderer(&self) -> RendererConfig<'_>"));
    assert!(facade.contains("pub fn theme(&self) -> ThemeConfig<'_>"));
    assert!(facade.contains("pub fn transition(&self) -> TransitionConfig<'_>"));
    assert!(facade.lines().count() <= 450, "config facade became a catch-all");
    for feature_key in [
        "keys::display::",
        "keys::matugen::",
        "keys::paper::",
        "keys::theme::",
        "keys::transition::",
        "keys::we_render::",
    ] {
        assert!(!facade.contains(feature_key), "config facade reclaimed {feature_key}");
    }
    for claimed_path in [
        "keys::paths::PAPER_BIN",
        "keys::paths::PAPER_STILL_BIN",
        "keys::paths::PAPER_VK_BIN",
        "paper_engine(",
        "wallpaper_mute(",
        "wallpaper_volume(",
    ] {
        assert!(!facade.contains(claimed_path), "config facade reclaimed {claimed_path}");
    }

    for (owner, marker, maximum) in [
        ("value/display.rs", "pub struct DisplayConfig", 100),
        ("value/renderer.rs", "pub struct RendererConfig", 160),
        ("value/theme.rs", "pub struct ThemeConfig", 240),
        ("value/transition.rs", "pub struct TransitionConfig", 90),
    ] {
        let source = std::fs::read_to_string(config.join(owner)).expect("read config owner");
        assert!(source.contains(marker), "{owner} lost {marker}");
        assert!(source.lines().count() <= maximum, "{owner} mixed responsibilities again");
        for raw_decode in ["serde_json::from_", "read_to_string", "Config::from_root"] {
            assert!(!source.contains(raw_decode), "{owner} reparses config through {raw_decode}");
        }
    }

    let production = production_rust_files(&config);
    let canonical_roots: Vec<_> = production
        .iter()
        .filter(|path| {
            std::fs::read_to_string(path)
                .expect("read config source")
                .contains("pub(super) root: Value")
        })
        .collect();
    assert_eq!(canonical_roots, [&config.join("value.rs")], "config root must stay canonical");
    let config_parsers: Vec<_> = production
        .iter()
        .filter(|path| {
            std::fs::read_to_string(path)
                .expect("read config source")
                .contains("serde_json::from_str")
        })
        .collect();
    assert_eq!(config_parsers, [&config.join("source.rs")], "config parsing must stay canonical");

    for (needle, owner) in [
        ("keys::paper::", "value/renderer.rs"),
        ("keys::we_render::", "value/renderer.rs"),
        ("keys::paths::PAPER_BIN", "value/renderer.rs"),
        ("keys::paths::PAPER_STILL_BIN", "value/renderer.rs"),
        ("keys::paths::PAPER_VK_BIN", "value/renderer.rs"),
        ("paper_engine(", "value/renderer.rs"),
        ("paper.awww.", "value/renderer.rs"),
        ("wallpaper_mute(", "value/renderer.rs"),
        ("wallpaper_volume(", "value/renderer.rs"),
        ("keys::transition::", "value/transition.rs"),
    ] {
        let matches: Vec<_> = production
            .iter()
            .filter(|path| {
                std::fs::read_to_string(path).expect("read config source").contains(needle)
            })
            .collect();
        assert_eq!(matches, [&config.join(owner)], "{needle} drifted from {owner}");
    }

    for source in production_rust_files(&root) {
        if source.starts_with(root.join("infrastructure/config")) {
            continue;
        }
        let body = std::fs::read_to_string(&source).expect("read production source");
        for raw_access in ["skwd_config::get(", "skwd_config::str_at("] {
            assert!(
                !body.contains(raw_access),
                "{} bypasses Config through {raw_access}",
                source.display()
            );
        }
    }
}

#[test]
fn wallpaper_apply_owners_are_explicit() {
    let root = source_root().join("infrastructure/wallpaper/apply");
    let owners = [
        ("orchestrator.rs", "fn apply_output("),
        ("engine.rs", "fn apply_static_override("),
        ("resolver.rs", "fn resolve_current_image("),
        ("policy.rs", "struct PaperPolicy"),
        ("lifecycle.rs", "fn record_and_dedup("),
        ("static_media.rs", "fn apply_static_owned("),
        ("video_media.rs", "fn apply_video("),
        ("wallpaper_engine.rs", "fn reload_we("),
        ("transition.rs", "enum TransitionSelection"),
        ("transaction.rs", "struct ReadyHandoff"),
        ("launch.rs", "struct RendererTransaction"),
        ("reconcile.rs", "struct PreparedBatch"),
        ("refresh.rs", "fn refresh_renderer_policy"),
    ];
    let production: Vec<(PathBuf, String)> = production_rust_files(&root)
        .into_iter()
        .map(|path| {
            let source = std::fs::read_to_string(&path).expect("read apply owner");
            (path, source)
        })
        .collect();

    for (owner, marker) in owners {
        let expected = root.join(owner);
        let matches: Vec<_> = production
            .iter()
            .filter(|(_, source)| source.contains(marker))
            .map(|(path, _)| path)
            .collect();
        assert_eq!(matches, vec![&expected], "{marker} must have exactly one owner");
    }

    let orchestrator =
        std::fs::read_to_string(root.join("orchestrator.rs")).expect("read apply map");
    assert!(orchestrator.lines().count() <= 140, "public apply map stopped being thin");
    for hidden_policy in [
        "RendererLaunchSpec",
        "Command::new",
        "paper_vk_bin",
        "video_swap",
        "managed_transition_args",
        "transition_override",
        "Option<(bool",
    ] {
        assert!(
            !orchestrator.contains(hidden_policy),
            "orchestrator reclaimed policy: {hidden_policy}"
        );
    }

    for (owner, maximum) in [
        ("engine.rs", 80),
        ("resolver.rs", 140),
        ("policy.rs", 260),
        ("lifecycle.rs", 180),
        ("static_media.rs", 540),
        ("video_media.rs", 540),
        ("wallpaper_engine.rs", 180),
        ("transition.rs", 320),
        ("reconcile.rs", 380),
        ("refresh.rs", 80),
    ] {
        let lines = std::fs::read_to_string(root.join(owner))
            .expect("read bounded apply owner")
            .lines()
            .count();
        assert!(lines <= maximum, "{owner} mixed responsibilities again: {lines} lines");
    }

    let resolver = std::fs::read_to_string(root.join("resolver.rs")).expect("read resolver");
    for side_effect in ["WallState", "renderers()", "Command::new", ".spawn("] {
        assert!(!resolver.contains(side_effect), "resolver owns side effect {side_effect}");
    }
    let policy = std::fs::read_to_string(root.join("policy.rs")).expect("read policy");
    for lifecycle in ["RendererLaunchSpec", "Command::new", ".spawn("] {
        assert!(!policy.contains(lifecycle), "policy owns lifecycle {lifecycle}");
    }

    // Dependency direction is part of ownership: orchestration/reconciliation
    // may call policy owners, but leaf policy owners never call back up.
    for owner in
        ["static_media.rs", "video_media.rs", "transition.rs", "policy.rs", "transaction.rs"]
    {
        let source = std::fs::read_to_string(root.join(owner)).expect("read apply leaf owner");
        for forbidden in ["super::orchestrator", "super::reconcile"] {
            assert!(!source.contains(forbidden), "{owner} introduced reverse edge {forbidden}");
        }
    }
    let transition =
        std::fs::read_to_string(root.join("transition.rs")).expect("read transition owner");
    for media in ["super::static_media", "super::video_media", "super::wallpaper_engine"] {
        assert!(!transition.contains(media), "transition owner executes media through {media}");
    }
    for owner in ["policy.rs", "transaction.rs"] {
        let source = std::fs::read_to_string(root.join(owner)).expect("read neutral owner");
        for media in ["super::static_media", "super::video_media", "super::wallpaper_engine"] {
            assert!(!source.contains(media), "{owner} depends on media owner {media}");
        }
    }

    let reconcile = std::fs::read_to_string(root.join("reconcile.rs")).expect("read reconcile");
    for media_policy in [
        "video_transition_args",
        "vk_video_args",
        "spawn_video_paper",
        "spawn_base_still",
        "MultiVideoEntry",
        "capture_transition_frame",
        "output_still_swap",
        "video_swap",
        "spawn_scene_for",
    ] {
        assert!(!reconcile.contains(media_policy), "reconcile hid media policy: {media_policy}");
    }

    let static_media =
        std::fs::read_to_string(root.join("static_media.rs")).expect("read static owner");
    assert!(!static_media.contains("wall_proto::kind::VIDEO"));
    assert!(!static_media.contains("wall_proto::kind::WE"));
    let video_media =
        std::fs::read_to_string(root.join("video_media.rs")).expect("read video owner");
    assert!(!video_media.contains("wall_proto::kind::STATIC"));
    assert!(!video_media.contains("wall_proto::kind::WE"));
}

#[test]
fn video_pipeline_owners_are_explicit_and_zero_copy_boundaries_stay_fixed() {
    let directory = source_root().join("infrastructure/media/video");
    let module_map = std::fs::read_to_string(directory.join("mod.rs")).expect("read video map");
    for owner in
        ["cancellation", "decoding", "extraction", "preview", "scaling", "source", "switching"]
    {
        assert!(module_map.contains(&format!("mod {owner};")), "missing video owner {owner}");
    }

    let source = std::fs::read_to_string(directory.join("source.rs")).expect("read source owner");
    assert!(source.contains("struct VideoSource"));
    assert!(source.contains("fn codec_context"));
    assert!(source.contains("fn frame_rate"));

    let extraction =
        std::fs::read_to_string(directory.join("extraction.rs")).expect("read extraction owner");
    assert!(extraction.contains("fn generate_video_thumbs"));
    assert!(extraction.contains("fn extract_frame_to"));

    let preview =
        std::fs::read_to_string(directory.join("preview.rs")).expect("read preview owner");
    assert!(preview.contains("fn generate_video_preview"));
    assert!(preview.contains("fn decode_preview_frames"));

    let decoding =
        std::fs::read_to_string(directory.join("decoding.rs")).expect("read decoding owner");
    assert!(decoding.contains("struct FramePipeline"));
    assert!(decoding.contains("enum FrameOutcome"));
    assert!(decoding.contains("DecoderReceive(ff::Error)"));
    assert!(decoding.contains("HardwareTransfer(i32)"));
    assert!(decoding.contains("struct PreviewPacer"));
    assert!(decoding.contains("fn open_persistent_decoder"));

    let cancellation = std::fs::read_to_string(directory.join("cancellation.rs"))
        .expect("read cancellation owner");
    assert!(cancellation.contains("struct Cancellation"));
    assert!(cancellation.contains("fn wait_until"));

    let switching =
        std::fs::read_to_string(directory.join("switching.rs")).expect("read switching owner");
    assert!(switching.contains("struct PersistState"));
    assert!(switching.contains("enum PlaybackEnd"));
    assert!(switching.contains("EndOfFile { frames: usize }"));
    assert!(switching.contains("Replaced,"));
    assert!(switching.contains("Cancelled,"));
    assert!(switching.contains("enum PlaybackFailure"));
    assert!(switching.contains("Demux(ff::Error)"));
    assert!(switching.contains("PacketSend(ff::Error)"));
    assert!(switching.contains("DecoderReceive(ff::Error)"));
    assert!(switching.contains("fn stream_source"));
    assert_eq!(switching.matches("std::thread::spawn").count(), 1);

    for owner in [&source, &extraction, &preview, &decoding, &cancellation, &switching] {
        assert!(!owner.contains("Box<dyn"));
        assert!(!owner.contains("Arc<dyn"));
        assert!(!owner.contains("std::process::Command"));
    }
    assert!(!module_map.contains("pipeline"));
    assert!(!directory.join("pipeline.rs").exists());
    assert!(!decoding.contains("rgba.clone"));
    assert!(!switching.contains("rgba.clone"));
}
