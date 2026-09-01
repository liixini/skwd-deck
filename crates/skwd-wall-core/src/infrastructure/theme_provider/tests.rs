#![cfg(test)]

use super::*;

fn doc() -> Value {
    crate::material::document_with("#854cff", true, "tonal-spot").unwrap()
}

#[test]
fn provider_payload_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let caelestia = dir.path().join("scheme.json");
    std::fs::write(
        &caelestia,
        br#"{"name":"dynamic","flavour":"default","mode":"dark","variant":"tonalspot","colours":{"term0":"000000"}}"#,
    )
    .unwrap();
    for provider in PROVIDERS {
        let value = payload(provider, &caelestia, &doc(), "tonal-spot")
            .unwrap_or_else(|| panic!("{provider} payload"));
        if provider == "caelestia" {
            assert_eq!(value["colours"]["term0"], "000000");
            assert_eq!(value["name"], "skwd-wall");
        }
        if provider == "end4" {
            assert_eq!(value.as_object().unwrap().len(), 49);
        }
        let canonical =
            normalize(provider, &value, true).unwrap_or_else(|| panic!("{provider} reverse"));
        assert_eq!(canonical.as_object().unwrap().len(), 29);
        assert_eq!(canonical["primaryText"], canonical["onPrimary"]);
    }
}

#[test]
fn noctalia_payload_modes() {
    let value = noctalia_payload(&doc()).unwrap();
    for mode in ["dark", "light"] {
        assert!(value[mode]["mPrimary"].as_str().unwrap().starts_with('#'));
        assert!(value[mode]["terminal"]["normal"]["red"].is_string());
        assert!(value[mode]["terminal"]["bright"]["cyan"].is_string());
    }
}

#[test]
fn unchanged_publish_noop() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config::from_root(json!({"paths": {"cache": dir.path()}}));
    let target = dir.path().join("target.json");
    assert!(write_payload(&cfg, "end4", &target, b"one").unwrap());
    assert!(!write_payload(&cfg, "end4", &target, b"one").unwrap());
    assert!(is_published_echo(&cfg, "end4", b"one"));
    assert!(!is_published_echo(&cfg, "end4", b"two"));
    assert!(write_payload(&cfg, "end4", &target, b"two").unwrap());
    assert_eq!(std::fs::read(target).unwrap(), b"two");
}

#[test]
fn invalid_palettes_rejected() {
    let mut value = end4_payload(&doc()).unwrap();
    value["primary"] = Value::String("not-a-colour".to_string());
    assert!(normalize("end4", &value, true).is_none());
    assert!(normalize("end4", &json!({"primary": "#ffffff"}), true).is_none());
    assert!(normalize("unknown", &value, true).is_none());
}

#[test]
fn provider_path_exact() {
    for provider in PROVIDERS {
        let path = provider_path(provider).unwrap();
        assert_eq!(provider_for_path(&path), Some(provider));
        assert_eq!(provider_for_path(&path.with_extension("json.tmp")), None);
    }
}

#[test]
fn end4_contract_detection() {
    let dir = tempfile::tempdir().unwrap();
    let matugen = dir.path().join("config.toml");
    let shell = dir.path().join("shell.qml");
    assert!(!end4_contract_available(true, &matugen, &shell));
    std::fs::write(&matugen, "[config]").unwrap();
    assert!(!end4_contract_available(true, &matugen, &shell));
    std::fs::write(&shell, "ShellRoot {}").unwrap();
    assert!(!end4_contract_available(false, &matugen, &shell));
    assert!(end4_contract_available(true, &matugen, &shell));
}
