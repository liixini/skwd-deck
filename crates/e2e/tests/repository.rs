use std::path::{Path, PathBuf};

const VERIFY_REVISION: &str = "43766a72f2d3f46072c68c4fb4735d9482bf5059";

fn root() -> PathBuf {
    workspace_from(&std::env::current_dir().expect("current directory"))
        .or_else(|| std::env::current_exe().ok().and_then(|executable| workspace_from(&executable)))
        .or_else(|| workspace_from(Path::new(env!("CARGO_MANIFEST_DIR"))))
        .expect("Deck workspace root")
}

fn workspace_from(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|candidate| {
            candidate.join("Cargo.toml").is_file()
                && candidate.join("crates/e2e/Cargo.toml").is_file()
        })
        .map(Path::to_path_buf)
}

fn read(path: &str) -> String {
    std::fs::read_to_string(root().join(path))
        .unwrap_or_else(|error| panic!("read {path}: {error}"))
}

#[test]
fn workspace_members() {
    let manifest = read("Cargo.toml");
    for member in [
        "crates/e2e",
        "crates/skwd-config",
        "crates/skwd-helm",
        "crates/skwd-log",
        "crates/skwd-palette",
        "crates/skwd-steam",
        "crates/skwd-wall-core",
        "crates/skwd-wall-effects",
        "crates/skwd-wall-scan",
        "crates/skwd-walld",
        "crates/wall-proto",
        "crates/wall-rules",
    ] {
        assert!(manifest.contains(&format!("\"{member}\"")), "missing {member}");
    }
    for removed in ["src", "assets", "locales", "shaders", "crates/paper-vk"] {
        assert!(!root().join(removed).exists(), "{removed} remains");
    }
}

#[test]
fn cross_repo_deps() {
    let core = read("crates/skwd-wall-core/Cargo.toml");
    let helm = read("crates/skwd-helm/Cargo.toml");
    let e2e = read("crates/e2e/Cargo.toml");
    let scanner = read("crates/skwd-wall-scan/Cargo.toml");
    let walld = read("crates/skwd-walld/Cargo.toml");
    let effects = read("crates/skwd-wall-effects/Cargo.toml");
    for manifest in [&core, &helm, &e2e] {
        assert!(manifest.contains("../../../skwd-paper/crates/paper-control"));
    }
    assert!(core.contains("../../../skwd-paper/crates/paper-geom"));
    assert!(core.contains("../skwd-palette"));
    assert!(scanner.contains("../../../skwd-paper/crates/paper-scene"));
    assert!(walld.contains("../../../skwd-lens/crates/skwd-lens-proto"));
    assert!(scanner.contains("../skwd-palette"));
    assert!(effects.contains("../skwd-palette"));

    let paper = read("crates/skwd-wall-core/src/infrastructure/paper/mod.rs");
    assert!(paper.contains("pub use paper_control::"));
    assert!(!root().join("crates/skwd-wall-core/src/infrastructure/paper/protocol.rs").exists());

    let protocol = read("crates/wall-proto/src/lib.rs");
    for paper_contract in [
        "PaperCommand",
        "StillCommand",
        "MultiVideoEntry",
        "OutputTarget",
        "signal_paper_ready",
        "spawn_stdin_line_reader",
        "VIDEO_EXTS",
        "is_video_path",
    ] {
        assert!(!protocol.contains(paper_contract), "wall-proto re-exports {paper_contract}");
    }
}

#[test]
fn ffmpeg_bindings_stock() {
    let manifest = read("Cargo.toml");
    assert!(!manifest.contains("ffmpeg-sys-the-third = { git"));
    assert!(!manifest.contains("skwd-ffmpeg-sys"));
    assert!(
        read("crates/skwd-wall-core/Cargo.toml")
            .contains("ffmpeg-the-third = { version = \"6\", optional = true")
    );

    let lock = read("Cargo.lock");
    let package = lock
        .split("[[package]]")
        .find(|entry| entry.contains("name = \"ffmpeg-sys-the-third\""))
        .expect("locked FFmpeg sys package");
    assert_eq!(package.matches("source = ").count(), 1);
    assert!(package.contains("version = \"6.0.0+ffmpeg-9.0\""));
    assert!(package.contains("source = \"registry+https://github.com/rust-lang/crates.io-index\""));
    assert!(!root().join("vendor/ffmpeg-sys-the-third").exists());

    let workflow = read(".forgejo/workflows/verify.yml");
    let private_checkouts = workflow
        .split("      - name: ")
        .skip(1)
        .map(|step| step.split("\n      - ").next().unwrap_or(step))
        .filter(|step| step.contains("repository: liixini/"))
        .collect::<Vec<_>>();
    assert_eq!(private_checkouts.len(), 3);
    for repository in ["skwd-paper", "skwd-lens", "skwd-verify"] {
        let checkout = private_checkouts
            .iter()
            .find(|step| step.contains(&format!("repository: liixini/{repository}")))
            .unwrap_or_else(|| panic!("missing private checkout for {repository}"));
        assert!(checkout.contains("token: ${{ secrets.SKWD_SUITE_READ_TOKEN }}"));
        assert!(checkout.contains("persist-credentials: false"));
    }
    assert_eq!(workflow.matches("token: ${{ secrets.SKWD_SUITE_READ_TOKEN }}").count(), 3);
    assert_eq!(workflow.matches("persist-credentials: false").count(), 4);
    assert!(
        workflow.contains(&format!(
            "repository: liixini/skwd-verify\n          ref: {VERIFY_REVISION}"
        ))
    );
    assert!(!workflow.contains("skwd-ffmpeg-sys"));
}

#[test]
fn ffmpeg_license_packaged() {
    let license = std::fs::read(root().join("LICENSES/ffmpeg-sys-the-third-WTFPL.txt"))
        .expect("retained FFmpeg sys license");
    let expected = concat!(
        "            DO WHAT THE FUCK YOU WANT TO PUBLIC LICENSE\n",
        "                    Version 2, December 2004\n",
        "\n",
        " Everyone is permitted to copy and distribute verbatim or modified\n",
        " copies of this license document, and changing it is allowed as long\n",
        " as the name is changed.\n",
        "\n",
        "            DO WHAT THE FUCK YOU WANT TO PUBLIC LICENSE\n",
        "   TERMS AND CONDITIONS FOR COPYING, DISTRIBUTION AND MODIFICATION\n",
        "\n",
        "  0. You just DO WHAT THE FUCK YOU WANT TO.\n",
    );
    assert_eq!(license, expected.as_bytes());

    let package_stage = read("scripts/package-stage.sh");
    assert!(package_stage.contains("LICENSES/ffmpeg-sys-the-third-WTFPL.txt"));
    assert!(package_stage.contains("ffmpeg-sys-the-third-LICENSE"));
    assert!(!package_stage.contains("vendor/ffmpeg-sys-the-third"));
    assert!(read("packaging/manifest.txt").contains("ffmpeg-sys-the-third-LICENSE"));
}

#[test]
fn package_identities() {
    let manifest = read("Cargo.toml");
    assert!(!manifest.contains("name = \"skwd\""));
    for package in [
        "skwd-wall-core",
        "skwd-walld",
        "skwd-wall-scan",
        "skwd-wall-effects",
        "skwd-steam",
        "skwd-helm",
    ] {
        let found = std::fs::read_dir(root().join("crates"))
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| std::fs::read_to_string(entry.path().join("Cargo.toml")).ok())
            .any(|contents| contents.contains(&format!("name = \"{package}\"")));
        assert!(found, "missing package identity {package}");
    }
    let socket = read("crates/wall-proto/src/socket.rs");
    assert!(socket.contains("join(\"skwd-wall-v2\").join(\"wall.sock\")"));
    let service = read("data/skwd-walld.service");
    assert!(service.contains("ExecStart=/usr/bin/skwd-walld --wait-for-session"));
    assert!(service.contains("Conflicts=skwd-daemon.service"));
}

#[test]
fn we_routing_native_only() {
    for path in [
        "crates/skwd-wall-core/src/infrastructure/we/adapter.rs",
        "crates/skwd-wall-core/src/infrastructure/wallpaper/apply/orchestrator.rs",
        "crates/skwd-wall-core/src/infrastructure/paper/composition.rs",
        "crates/skwd-walld/src/composition/apply.rs",
    ] {
        let source = read(path);
        assert!(!source.contains("linux-wallpaperengine"), "external WE route in {path}");
        assert!(!source.contains("we_scene_engine"), "legacy WE selector in {path}");
    }
    let reaper = read("crates/skwd-walld/src/infrastructure/reap.rs");
    assert!(reaper.contains("linux-wallpaperengine"));
    assert!(!reaper.contains("Command::new"));
}
