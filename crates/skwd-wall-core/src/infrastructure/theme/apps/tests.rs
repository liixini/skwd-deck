use super::{catalogue::RECIPES, files, manager};
use crate::config::Config;
use serde_json::{Value, json};

fn fixture() -> (tempfile::TempDir, manager::Environment, Config) {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for app in &RECIPES {
        let path = bin.join(app.id);
        std::fs::write(&path, "").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let env = manager::Environment {
        home: root.path().join("home"),
        config: root.path().join("config"),
        data: root.path().join("data"),
        data_dirs: Vec::new(),
        receipts: root.path().join("state"),
        search: vec![bin],
        reload: false,
    };
    (root, env, Config::from_root(json!({})))
}

fn palette(color: &str) -> Value {
    let mut value = json!({});
    for key in super::super::profiles::ROLE_KEYS {
        value[key] = json!(color);
    }
    value
}

#[test]
fn every_recipe_enables_updates_and_restores_unrelated_edits() {
    let (_root, env, config) = fixture();
    for recipe in &RECIPES {
        let path = env.config.join(recipe.config);
        let original = if recipe.id == "btop" {
            "update_ms = 2000\ncolor_theme = \"Default\"\n"
        } else if recipe.id == "niri" {
            "layout { gaps 12; }\n"
        } else {
            "font-size = 14\n"
        };
        files::write(&path, original).unwrap();
        manager::set_with(&env, &config, recipe.id, true, &palette("#123456"), true).unwrap();
        let statuses = manager::list_with(&env, &config);
        let status = statuses.apps.iter().find(|app| app.id == recipe.id).unwrap();
        assert!(status.enabled && status.can_disable, "{status:?}");
        let first = std::fs::read_to_string(&status.output_path).unwrap();
        assert!(first.contains("#123456"));
        assert!(!first.contains("{{"));
        let enabled = std::fs::read_to_string(&path).unwrap();
        manager::set_with(&env, &config, recipe.id, true, &palette("#123456"), true).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), enabled);
        std::fs::write(&path, format!("{enabled}\n# later edit\n")).unwrap();
        manager::apply_with(&env, &config, &palette("#abcdef"), false);
        assert!(std::fs::read_to_string(&status.output_path).unwrap().contains("#abcdef"));
        manager::set_with(&env, &config, recipe.id, false, &Value::Null, true).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), format!("{original}\n# later edit\n"));
        assert!(!std::path::Path::new(&status.output_path).exists());
        manager::apply_with(&env, &config, &palette("#fedcba"), true);
        assert!(!std::path::Path::new(&status.output_path).exists());
    }
}

#[test]
fn absent_configs_are_created_and_removed_on_disable() {
    let (_root, env, config) = fixture();
    for recipe in RECIPES.iter().filter(|recipe| recipe.id != "waybar") {
        manager::set_with(&env, &config, recipe.id, true, &palette("#123456"), true).unwrap();
        manager::set_with(&env, &config, recipe.id, false, &Value::Null, true).unwrap();
        assert!(!env.config.join(recipe.config).exists());
    }
}

#[test]
fn external_theme_edits_stop_updates_and_are_preserved_on_disable() {
    let (_root, env, config) = fixture();
    manager::set_with(&env, &config, "kitty", true, &palette("#123456"), true).unwrap();
    let output = env.config.join("kitty/skwd-colors.conf");
    std::fs::write(&output, "user theme").unwrap();
    manager::apply_with(&env, &config, &palette("#abcdef"), true);
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "user theme");
    let status =
        manager::list_with(&env, &config).apps.into_iter().find(|app| app.id == "kitty").unwrap();
    assert_eq!(status.state, "changed");
    manager::set_with(&env, &config, "kitty", false, &Value::Null, true).unwrap();
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "user theme");
}

#[test]
fn rejects_symlinks_conflicts_and_invalid_palettes_without_writing() {
    let (root, env, config) = fixture();
    let target = root.path().join("declarative");
    files::write(&target, "font_size 12\n").unwrap();
    std::fs::create_dir_all(env.config.join("kitty")).unwrap();
    std::os::unix::fs::symlink(&target, env.config.join("kitty/kitty.conf")).unwrap();
    assert!(manager::set_with(&env, &config, "kitty", true, &palette("#123456"), true).is_err());
    assert_eq!(std::fs::read_to_string(target).unwrap(), "font_size 12\n");
    files::write(&env.config.join("ghostty/config"), "config-file = noctalia.conf\n").unwrap();
    assert!(manager::set_with(&env, &config, "ghostty", true, &palette("#123456"), true).is_err());
    assert!(manager::set_with(&env, &config, "btop", true, &Value::Null, true).is_err());
    assert!(!env.config.join("btop/btop.conf").exists());
    assert!(manager::set_with(&env, &config, "../kitty", true, &palette("#123456"), true).is_err());
}

#[test]
fn changed_theme_selection_is_not_overwritten_when_disabling() {
    let (_root, env, config) = fixture();
    files::write(&env.config.join("btop/btop.conf"), "color_theme = \"Default\"\n").unwrap();
    manager::set_with(&env, &config, "btop", true, &palette("#123456"), true).unwrap();
    let path = env.config.join("btop/btop.conf");
    std::fs::write(&path, "color_theme = \"TTY\"\n").unwrap();
    assert!(manager::set_with(&env, &config, "btop", false, &Value::Null, true).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "color_theme = \"TTY\"\n");
}

#[test]
fn interrupted_setup_can_be_undone_before_or_after_config_write() {
    for published in [false, true] {
        let (_root, env, config) = fixture();
        manager::set_with(&env, &config, "kitty", true, &palette("#123456"), true).unwrap();
        let path = env.receipts.join("kitty.json");
        let mut receipt = files::load(&path).unwrap().unwrap();
        receipt.enabled = false;
        receipt.pending = true;
        if !published {
            std::fs::remove_file(&receipt.config).unwrap();
        }
        files::save(&path, &receipt).unwrap();
        let row = manager::list_with(&env, &config)
            .apps
            .into_iter()
            .find(|app| app.id == "kitty")
            .unwrap();
        assert!(row.can_disable, "{row:?}");
        manager::set_with(&env, &config, "kitty", false, &Value::Null, true).unwrap();
        assert!(!receipt.output.exists());
        assert!(!receipt.config.exists());
    }
}

#[test]
fn disabled_custom_outputs_are_retained_but_never_rendered() {
    let (root, env, _) = fixture();
    let output = root.path().join("old-theme.conf");
    let config = Config::from_root(json!({"integrations": [
        {"name": "kitty", "template": "kitty.conf", "output": output, "enabled": false},
        {"name": "other", "template": "custom.conf", "output": "/other"}
    ]}));
    assert!(config.theme().disabled_integration("kitty"));
    assert_eq!(config.theme().integrations().len(), 1);
    files::write(&env.config.join("kitty/kitty.conf"), "include matugen.conf\n").unwrap();
    manager::set_with(&env, &config, "kitty", true, &palette("#123456"), true).unwrap();
    assert!(!output.exists());
    assert_eq!(skwd_config::schema::read_boolean(&json!({}), "integrations.0.enabled"), Some(true));
}

#[test]
fn waybar_without_a_stylesheet_is_held_for_review() {
    let (_root, env, config) = fixture();
    let status =
        manager::list_with(&env, &config).apps.into_iter().find(|app| app.id == "waybar").unwrap();
    assert_eq!(status.state, "needs-review");
    assert!(!status.can_enable);
    assert!(manager::set_with(&env, &config, "waybar", true, &palette("#123456"), true).is_err());
    assert!(!env.config.join("waybar").exists());
}

#[test]
fn stylesheet_apps_use_their_own_comment_syntax_after_existing_theme_lines() {
    let (_root, env, config) = fixture();
    let rofi = env.config.join("rofi/config.rasi");
    let waybar = env.config.join("waybar/style.css");
    files::write(&rofi, "configuration { show-icons: true; }\n@theme \"arthur\"\n").unwrap();
    files::write(&waybar, "@import \"base.css\";\nwindow#waybar { color: @primary; }\n").unwrap();
    manager::set_with(&env, &config, "rofi", true, &palette("#123456"), true).unwrap();
    manager::set_with(&env, &config, "waybar", true, &palette("#123456"), true).unwrap();
    let rasi = std::fs::read_to_string(&rofi).unwrap();
    let css = std::fs::read_to_string(&waybar).unwrap();
    assert!(rasi.ends_with(
        "@theme \"arthur\"\n// Skwd app theme\n@import \"skwd-colors.rasi\"\n// End Skwd app theme\n"
    ));
    assert!(css.ends_with(
        "/* Skwd app theme */\n@import \"skwd-colors.css\";\n/* End Skwd app theme */\n"
    ));
    assert!(!rasi.contains('#'));
    assert!(!css.contains("\n#") && !css.contains("\n//"));
    let colors = std::fs::read_to_string(env.config.join("waybar/skwd-colors.css")).unwrap();
    assert!(colors.contains("@define-color primary #123456;"));
    assert!(colors.contains("@define-color ansi_magenta_bright #"));
    let rendered = std::fs::read_to_string(env.config.join("rofi/skwd-colors.rasi")).unwrap();
    assert!(rendered.contains("selected-normal-background:  #123456;"));
}

#[test]
fn apps_without_a_resident_process_report_next_open() {
    let (_root, mut env, _) = fixture();
    env.reload = true;
    let rofi = RECIPES.iter().find(|recipe| recipe.id == "rofi").unwrap();
    assert_eq!(super::reload::reload(&env, rofi), "on-next-open");
}
