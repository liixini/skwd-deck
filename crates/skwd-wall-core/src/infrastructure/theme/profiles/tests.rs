use super::*;
use std::fmt::Write;

fn palette() -> Value {
    Value::Object(ROLE_KEYS.into_iter().map(|key| (key.to_string(), json!("#123456"))).collect())
}

#[test]
fn profiles_follow_only_the_matching_source_and_variant() {
    let mut root = json!({"theme": {"policy": "wallpaper", "wallpaperProfiles": [
        {"key": "wallpaper-x", "enabled": true, "dark": palette()}
    ]}});
    let config = Config::from_root(root.clone());
    assert_eq!(selected(&config, "wallpaper-x", true), Some(palette()));
    assert!(selected(&config, "wallpaper-y", true).is_none());
    assert!(selected(&config, "wallpaper-x", false).is_none());
    root["theme"]["wallpaperProfiles"][0]["enabled"] = json!(false);
    assert!(selected(&Config::from_root(root.clone()), "wallpaper-x", true).is_none());
    root["theme"]["wallpaperProfiles"][0]["enabled"] = json!(true);
    for policy in ["fixed", "off"] {
        root["theme"]["policy"] = json!(policy);
        assert!(selected(&Config::from_root(root.clone()), "wallpaper-x", true).is_none());
    }
}

#[test]
fn invalid_and_incomplete_profiles_are_rejected() {
    let mut value = palette();
    assert!(valid_palette(&value));
    value["primary"] = json!("#12345z");
    assert!(!valid_palette(&value));
    assert!(!valid_palette(&json!({"primary": "#123456"})));
}

#[test]
fn profile_identity_survives_a_thumbnail_replacement() {
    let state = WallState::test_new(
        json!({"theme": {"policy": "wallpaper", "mode": "dark", "wallpaperProfiles": [
            {"key": "wallpaper-x", "enabled": true, "dark": palette()}
        ]}}),
    );
    state.with_db(|db| db.execute("INSERT INTO meta (key, name, type, thumb) VALUES ('wallpaper-x', 'Wallpaper X', 'video', '/old.jpg')", [])).unwrap();
    assert_eq!(super::palette(&state, "/old.jpg"), Some(palette()));
    state
        .with_db(|db| {
            db.execute("UPDATE meta SET thumb = '/new.jpg' WHERE key = 'wallpaper-x'", [])
        })
        .unwrap();
    assert_eq!(super::palette(&state, "/new.jpg"), Some(palette()));
    assert!(super::palette(&state, "/other.jpg").is_none());
}

#[test]
fn current_snapshot_ignores_hover_and_pending_wallpaper() {
    let directory = tempfile::tempdir().unwrap();
    let state = WallState::test_new(
        json!({"paths": {"cache": directory.path()}, "theme": {"mode": "dark"}}),
    );
    let path = directory.path().join("colors.json");
    std::fs::write(&path, palette().to_string()).unwrap();
    remember_applied(&state, "/applied.jpg").unwrap();
    state.theme().set_source("/pending.jpg");
    std::fs::write(&path, "{}").unwrap();
    let snapshot = current(&state).unwrap();
    assert_eq!(snapshot["source"], "/applied.jpg");
    assert_eq!(snapshot["palette"], palette());
}

#[test]
fn publishing_keeps_exact_edits_in_integrations() {
    let directory = tempfile::tempdir().unwrap();
    let template = directory.path().join("exact.txt");
    let output = directory.path().join("result.txt");
    std::fs::write(&template, "{{colors.primary.default.hex}} {{colors.surface.default.hex}}")
        .unwrap();
    let config = Config::from_root(
        json!({"paths": {"cache": directory.path(), "templates": directory.path()},
        "theme": {"style": "pastel"}, "integrations": [{"template": "exact.txt", "output": output}]}),
    );
    let mut edited = palette();
    edited["surface"] = json!("#abcdef");
    assert!(publish(&config, &edited, true));
    assert_eq!(std::fs::read_to_string(output).unwrap(), "#123456 #abcdef");
    let applied: Value =
        serde_json::from_slice(&std::fs::read(directory.path().join("colors.json")).unwrap())
            .unwrap();
    assert_eq!(applied, edited);
}

#[test]
fn all_edited_roles_reach_templates_and_the_current_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let template = directory.path().join("full.txt");
    let mut doc = crate::material::document("#84d1ce", true).unwrap();
    let mut text = String::new();
    let mut expected = String::new();
    for (index, key) in crate::material::ROLE_KEYS.iter().enumerate() {
        for (mode, base) in [("dark", 0x0012_3400), ("light", 0x00ab_cd00)] {
            let hex = format!("#{:06x}", base + index);
            doc["colors"][key][mode]["color"] = json!(hex);
            writeln!(text, "{{{{colors.{key}.{mode}.hex}}}}").unwrap();
            writeln!(expected, "{hex}").unwrap();
        }
    }
    text.push_str("{{colors.secondary.default.hex}}\n{{base16.base08.hex}}");
    std::fs::write(&template, text).unwrap();
    let state = WallState::test_new(json!({
        "paths": {"cache": directory.path(), "templates": directory.path()},
        "theme": {"mode": "light", "style": "pastel"},
        "integrations": [{"template": "full.txt", "output": "full.out"}]
    }));
    let config = state.config().clone();
    let mut palette = palette();
    palette["_scheme"] = doc.clone();
    palette["_schemeVersion"] = json!(1);
    assert!(publish(&config, &palette, false));
    let secondary = crate::material::role(&doc, "secondary", "light").unwrap();
    let error = crate::material::role(&doc, "error", "light").unwrap();
    write!(expected, "{secondary}\n{error}").unwrap();
    assert_eq!(std::fs::read_to_string(directory.path().join("full.out")).unwrap(), expected);
    remember_applied(&state, "/wallpaper.jpg").unwrap();
    let current = current(&state).unwrap();
    for key in crate::material::ROLE_KEYS {
        for mode in ["dark", "light"] {
            assert_eq!(current["scheme"]["colors"][key][mode], doc["colors"][key][mode]);
        }
        assert_eq!(current["scheme"]["colors"][key]["default"], doc["colors"][key]["light"]);
    }
    assert_eq!(current["palette"]["primary"], doc["colors"]["primary"]["light"]["color"]);
}
