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
fn pinned_variant_reaches_apply_and_preview_without_mutating_global_settings() {
    let directory = tempfile::tempdir().unwrap();
    let mut light = palette();
    light["primary"] = json!("#abcdef");
    let state = WallState::test_new(json!({
        "paths": {"cache": directory.path()},
        "theme": {"policy": "wallpaper", "mode": "dark", "wallpaperProfiles": [{
            "key": "wallpaper-x", "enabled": true, "dark": palette(), "light": light,
            "settingsPinned": true, "settings": {"theme.mode": "light"}
        }]}
    }));
    state.with_db(|db| db.execute(
        "INSERT INTO meta (key, name, type, thumb) VALUES ('wallpaper-x', 'X', 'static', '/x.jpg')", []
    )).unwrap();
    assert_eq!(super::palette(&state, "/x.jpg"), Some(light.clone()));
    assert_eq!(crate::bridge_preview::cached_palette(&state, "/x.jpg"), Some(light));
    assert!(apply(&state, "/x.jpg", &configuration(&state, "/x.jpg")));
    remember_applied(&state, "/x.jpg", &configuration(&state, "/x.jpg")).unwrap();
    let applied = current(&state).unwrap();
    assert_eq!(applied["palette"]["primary"], "#abcdef");
    assert_eq!(applied["dark"], false);
    assert_eq!(state.config().theme().mode(), "dark");
}

#[test]
fn weather_palette_overrides_policy() {
    let directory = tempfile::tempdir().unwrap();
    let state = WallState::test_new(json!({
        "paths": {"cache": directory.path()},
        "theme": {"policy": "off", "mode": "dark", "wallpaperProfiles": [{
            "key": "wallpaper-x", "enabled": true, "dark": palette(),
            "settingsPinned": true, "settings": {"theme.policy": "fixed"}
        }]}
    }));
    state.with_db(|db| db.execute(
        "INSERT INTO meta (key, name, type, thumb) VALUES ('wallpaper-x', 'X', 'static', '/x.jpg')", []
    )).unwrap();
    assert_eq!(configuration(&state, "/x.jpg").theme().policy(), "fixed");
    let config = configuration(&state, "/x.jpg")
        .with_override(skwd_config::keys::theme::POLICY, json!("wallpaper"));
    assert!(apply(&state, "/x.jpg", &config));
    remember_applied(&state, "/x.jpg", &config).unwrap();
    assert_eq!(current(&state).unwrap()["palette"]["primary"], "#123456");
    assert_eq!(state.config().theme().policy(), "off");
}

#[test]
fn current_snapshot_ignores_hover_and_pending_wallpaper() {
    let directory = tempfile::tempdir().unwrap();
    let state = WallState::test_new(
        json!({"paths": {"cache": directory.path()}, "theme": {"mode": "dark"}}),
    );
    let path = directory.path().join("colors.json");
    std::fs::write(&path, palette().to_string()).unwrap();
    remember_applied(&state, "/applied.jpg", &configuration(&state, "/applied.jpg")).unwrap();
    state.theme().set_source("/pending.jpg");
    std::fs::write(&path, "{}").unwrap();
    let snapshot = current(&state).unwrap();
    assert_eq!(snapshot["source"], "/applied.jpg");
    assert_eq!(snapshot["palette"], palette());
}

#[test]
fn remembers_backend_roles() {
    let dir = tempfile::tempdir().unwrap();
    let template = "{{colors.on_primary.default.hex}} {{colors.on_surface.default.hex}} {{colors.surface_variant.default.hex}} {{colors.surface_container.default.hex}}";
    for primary_text in [None, Some("#abcdef")] {
        let state = WallState::test_new(json!({"paths": {"cache": dir.path()}}));
        let mut colors = json!({
            "primary": "#111111", "tertiary": "#222222", "surface_variant": "#333333",
            "surface_container": "#444444", "surface": "#555555", "outline": "#666666",
            "background": "#777777", "on_primary": "#aaaaaa", "on_surface": "#bbbbbb"
        });
        if let Some(text) = primary_text {
            colors["primaryText"] = json!(text);
        }
        std::fs::write(dir.path().join("colors.json"), colors.to_string()).unwrap();
        remember_applied(&state, "/applied.png", &configuration(&state, "/applied.png")).unwrap();
        let snapshot = current(&state).unwrap();
        let applied = &snapshot["palette"];
        let text = primary_text.unwrap_or("#aaaaaa");
        assert_eq!(applied["primaryText"], text);
        assert_eq!(applied["surfaceText"], "#bbbbbb");
        assert_eq!(applied["surfaceVariant"], "#333333");
        assert_eq!(applied["surfaceContainer"], "#444444");
        assert_eq!(applied["on_primary"], "#aaaaaa");
        assert_eq!(
            crate::static_templates::render(
                template,
                &crate::static_templates::material_map(applied, true)
            ),
            format!("{text} #bbbbbb #333333 #444444")
        );
    }
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
    remember_applied(&state, "/wallpaper.jpg", &configuration(&state, "/wallpaper.jpg")).unwrap();
    let current = current(&state).unwrap();
    for key in crate::material::ROLE_KEYS {
        for mode in ["dark", "light"] {
            assert_eq!(current["scheme"]["colors"][key][mode], doc["colors"][key][mode]);
        }
        assert_eq!(current["scheme"]["colors"][key]["default"], doc["colors"][key]["light"]);
    }
    assert_eq!(current["palette"]["primary"], doc["colors"]["primary"]["light"]["color"]);
}

#[test]
fn matugen_snapshot_keeps_the_resolved_smart_mode() {
    let directory = tempfile::tempdir().unwrap();
    let state = WallState::test_new(json!({
        "paths": {"cache": directory.path()},
        "theme": {"policy": "wallpaper", "authority": "skwd", "engine": "matugen", "mode": "smart"}
    }));
    for dark in [false, true] {
        let document = crate::material::document_with("#854cff", dark, "tonal-spot").unwrap();
        let palette = crate::material::ui_palette(&document).unwrap();
        std::fs::write(directory.path().join("colors.json"), palette.to_string()).unwrap();
        std::fs::write(directory.path().join("scheme.json"), document.to_string()).unwrap();
        remember_applied(&state, "/smart.jpg", &configuration(&state, "/smart.jpg")).unwrap();
        let snapshot = current(&state).unwrap();
        assert_eq!(snapshot["dark"], dark);
        assert_eq!(snapshot["palette"], palette);
        assert_eq!(state.config().theme().mode(), "smart");
    }
}
