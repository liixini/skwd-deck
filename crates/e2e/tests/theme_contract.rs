use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use skwd_wall_core::{config::Config, material, theme_provider};

const SEED: &str = "#42ff77";
const SCHEME: &str = "tonal-spot";
const ROOT_ENV: &str = "SKWD_THEME_CONTRACT_ROOT";
const UPDATE_ENV: &str = "SKWD_UPDATE_GOLDENS";
const CAELESTIA_SEED: &str =
    r#"{"name":"native","flavour":"default","mode":"dark","variant":"tonalspot","colours":{}}"#;

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/theme-provider")
}

fn config(root: &Path) -> Config {
    Config::from_root(json!({
        "paths": {"cache": root.join("cache"), "noctaliaBin": root.join("missing-noctalia")},
        "theme": {"authority": "skwd", "scheme": SCHEME, "targets": theme_provider::PROVIDERS}
    }))
}

fn compare(name: &str, actual: &Value) {
    let path = golden_dir().join(format!("{name}.json"));
    let pretty = format!("{}\n", serde_json::to_string_pretty(actual).unwrap());
    if std::env::var_os(UPDATE_ENV).is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, pretty).unwrap();
        return;
    }
    let golden = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}; run with {UPDATE_ENV}=1", path.display()));
    assert_eq!(
        pretty,
        golden,
        "{} drifted from the reviewed contract; rerun with {UPDATE_ENV}=1 only for a deliberate change",
        path.display()
    );
}

fn check_contract(root: &Path) {
    let document = material::document_with(SEED, true, SCHEME).unwrap();
    let caelestia = theme_provider::provider_path("caelestia").unwrap();
    std::fs::create_dir_all(caelestia.parent().unwrap()).unwrap();
    std::fs::write(&caelestia, format!("{CAELESTIA_SEED}\n")).unwrap();
    let cfg = config(root);
    theme_provider::publish(&cfg, &document);
    compare("canonical-scheme", &document);
    for provider in theme_provider::PROVIDERS {
        let path = theme_provider::provider_path(provider).unwrap();
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("{provider}: {}: {error}", path.display()));
        let native: Value = serde_json::from_slice(&bytes).unwrap();
        compare(provider, &native);
        assert!(theme_provider::is_published_echo(&cfg, provider, &bytes), "{provider} origin");
        for (mode, dark) in [("dark", true), ("light", false)] {
            let canonical = theme_provider::normalize(provider, &native, dark)
                .unwrap_or_else(|| panic!("{provider} {mode} normalize"));
            assert_eq!(canonical.as_object().unwrap().len(), 29, "{provider} {mode}");
            compare(&format!("{provider}.canonical-{mode}"), &canonical);
        }
    }
    let listed: Vec<String> = std::fs::read_dir(golden_dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let mut expected = vec!["canonical-scheme.json".to_string()];
    for provider in theme_provider::PROVIDERS {
        expected.push(format!("{provider}.json"));
        expected.push(format!("{provider}.canonical-dark.json"));
        expected.push(format!("{provider}.canonical-light.json"));
    }
    let mut listed = listed;
    listed.sort();
    expected.sort();
    assert_eq!(listed, expected, "golden directory must hold exactly the provider contract");
}

#[test]
fn theme_provider_contract_goldens() {
    if let Ok(root) = std::env::var(ROOT_ENV) {
        check_contract(Path::new(&root));
        return;
    }
    let dir = std::env::temp_dir().join(format!("skwd-theme-contract-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("cache")).unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "theme_provider_contract_goldens", "--nocapture"])
        .env(ROOT_ENV, &dir)
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .env("XDG_DATA_HOME", dir.join("data"))
        .env("XDG_STATE_HOME", dir.join("state"))
        .env_remove("NOCTALIA_CONFIG_HOME")
        .env_remove("SKWD_WALL_V2_CONFIG")
        .env_remove("SKWD_WALL_V2_CACHE");
    if let Some(update) = std::env::var_os(UPDATE_ENV) {
        command.env(UPDATE_ENV, update);
    }
    let out = command.output().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
