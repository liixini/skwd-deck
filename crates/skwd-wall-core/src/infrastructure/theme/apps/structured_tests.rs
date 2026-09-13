use super::*;
use serde_json::json;

fn fixture() -> (tempfile::TempDir, Environment, crate::config::Config) {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    for app in &APPS {
        let path = bin.join(app.id);
        std::fs::write(&path, "").unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let env = Environment {
        home: root.path().join("home"),
        config: root.path().join("config"),
        data: root.path().join("data"),
        data_dirs: vec![],
        receipts: root.path().join("state"),
        search: vec![bin],
        reload: false,
    };
    (root, env, crate::config::Config::from_root(json!({})))
}

fn palette(color: &str) -> Value {
    let mut value = json!({});
    for key in super::super::super::profiles::ROLE_KEYS {
        value[key] = json!(color);
    }
    value
}

fn original(app: &App) -> &'static str {
    match app.id {
        "code" => {
            "{\n// Keep this comment\n\"editor.fontSize\": 17,\n\"workbench.colorCustomizations\": {\"editor.background\": \"#112233\", \"editor.lineHighlightBackground\": \"#445566\"},\n}\n"
        }
        "alacritty" => {
            "# Keep this comment\n[font]\nsize = 17\n[colors.primary]\nbackground = '#112233'\n"
        }
        _ => {
            "# Keep this comment\n[mgr]\ncwd = { fg = '#112233', bold = true }\n[icon]\ndirs = [{ name = 'Documents', text = 'D' }]\n"
        }
    }
}

#[test]
fn every_app_updates_and_restores_original_bytes() {
    let (_root, env, config) = fixture();
    for app in &APPS {
        let path = app.paths(&env).0;
        files::write(&path, original(app)).unwrap();
        app.set(&env, &config, true, &palette("#123456"), true).unwrap();
        let status = app.inspect(&env, &config);
        assert!(status.enabled);
        assert_eq!(status.state, if app.id == "yazi" { "on-next-open" } else { "watching" });
        app.set(&env, &config, true, &palette("#abcdef"), false).unwrap();
        let text = files::read(&path).unwrap().unwrap();
        assert!(text.contains("Keep this comment") && text.contains("#abcdef"));
        app.set(&env, &config, false, &Value::Null, true).unwrap();
        assert_eq!(files::read(&path).unwrap().unwrap(), original(app));
    }
}

#[test]
fn unrelated_edits_survive_but_user_colour_edits_stop_updates() {
    let (_root, env, config) = fixture();
    for app in &APPS {
        let path = app.paths(&env).0;
        files::write(&path, original(app)).unwrap();
        app.set(&env, &config, true, &palette("#123456"), true).unwrap();
        let text = files::read(&path).unwrap().unwrap();
        let mut doc = Document::parse(&text, app.json).unwrap();
        let key = if app.json {
            vec!["extra_setting".into()]
        } else {
            vec!["extra".into(), "setting".into()]
        };
        doc.set(&key, Some("42")).unwrap();
        files::write(&path, &doc.text()).unwrap();
        app.set(&env, &config, true, &palette("#abcdef"), false).unwrap();
        app.set(&env, &config, false, &Value::Null, true).unwrap();
        let restored = files::read(&path).unwrap().unwrap();
        assert!(restored.contains("#112233") && restored.contains("Keep this comment"));
        assert_eq!(
            Document::parse(&restored, app.json).unwrap().get(&key).unwrap().unwrap().trim(),
            "42"
        );
        app.set(&env, &config, true, &palette("#123456"), true).unwrap();
        let edited = files::read(&path).unwrap().unwrap().replacen("#123456", "#fedcba", 1);
        files::write(&path, &edited).unwrap();
        assert!(app.set(&env, &config, true, &palette("#abcdef"), false).is_err());
        assert!(app.set(&env, &config, false, &Value::Null, true).is_err());
        assert_eq!(files::read(&path).unwrap().unwrap(), edited);
        assert_eq!(app.inspect(&env, &config).state, "changed");
    }
}

#[test]
fn missing_configs_are_removed_and_invalid_configs_are_untouched() {
    let (_root, env, config) = fixture();
    for app in &APPS {
        let path = app.paths(&env).0;
        app.set(&env, &config, true, &palette("#123456"), true).unwrap();
        app.set(&env, &config, false, &Value::Null, true).unwrap();
        assert!(!path.exists());
        files::write(&path, "{ invalid").unwrap();
        assert!(app.set(&env, &config, true, &palette("#123456"), true).is_err());
        assert_eq!(files::read(&path).unwrap().unwrap(), "{ invalid");
    }
}

#[test]
fn duplicate_json_settings_and_symlinks_are_not_modified() {
    let (root, env, config) = fixture();
    let app = &APPS[0];
    let path = app.paths(&env).0;
    let original = "{\"editor.fontSize\": 12, \"editor.fontSize\": 14}";
    files::write(&path, original).unwrap();
    assert!(app.set(&env, &config, true, &palette("#123456"), true).is_err());
    assert_eq!(files::read(&path).unwrap().unwrap(), original);
    std::fs::remove_file(&path).unwrap();
    let target = root.path().join("declarative");
    files::write(&target, "{}").unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    assert!(app.set(&env, &config, true, &palette("#123456"), true).is_err());
    assert_eq!(files::read(&target).unwrap().unwrap(), "{}");
}

#[test]
fn interrupted_writes_restore_without_dropping_unrelated_edits() {
    for published in [false, true] {
        let (_root, env, config) = fixture();
        for app in &APPS {
            let path = app.paths(&env).0;
            files::write(&path, original(app)).unwrap();
            app.set(&env, &config, true, &palette("#123456"), true).unwrap();
            let text = files::read(&path).unwrap().unwrap();
            let edited = format!("{text}\n{} later edit\n", if app.json { "//" } else { "#" });
            files::write(&path, &edited).unwrap();
            app.set(&env, &config, true, &palette("#abcdef"), true).unwrap();
            let mut receipt = app.load(&env).unwrap().unwrap();
            receipt.pending = true;
            files::write(&app.paths(&env).1, &serde_json::to_string(&receipt).unwrap()).unwrap();
            if !published {
                files::write(&path, receipt.pending_before.as_ref().unwrap()).unwrap();
            }
            assert!(app.inspect(&env, &config).can_disable);
            app.set(&env, &config, false, &Value::Null, true).unwrap();
            let restored = files::read(&path).unwrap().unwrap();
            assert!(restored.contains("later edit") && restored.contains("#112233"));
        }
    }
}

#[test]
fn interrupted_restoration_is_recoverable_before_and_after_file_write() {
    for published in [false, true] {
        let (_root, env, config) = fixture();
        for app in &APPS {
            let path = app.paths(&env).0;
            files::write(&path, original(app)).unwrap();
            app.set(&env, &config, true, &palette("#123456"), true).unwrap();
            let enabled = files::read(&path).unwrap().unwrap();
            app.set(&env, &config, false, &Value::Null, true).unwrap();
            let mut receipt = app.load(&env).unwrap().unwrap();
            receipt.pending = true;
            receipt.enabled = true;
            files::write(&app.paths(&env).1, &serde_json::to_string(&receipt).unwrap()).unwrap();
            if !published {
                files::write(&path, &enabled).unwrap();
            }
            assert!(app.inspect(&env, &config).can_disable);
            app.set(&env, &config, false, &Value::Null, true).unwrap();
            assert_eq!(files::read(&path).unwrap().unwrap(), original(app));
        }
    }
}
