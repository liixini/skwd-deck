use super::super::{
    manager,
    tests::{fixture, palette},
};
use super::*;

#[test]
fn every_recipe_uses_custom_templates_and_reset_keeps_a_backup() {
    let (_root, env, config) = fixture();
    for recipe in &catalogue::RECIPES {
        files::write(&env.config.join(recipe.config), "").unwrap();
        edit_template(&env, recipe.id, false).unwrap();
        let custom = template(&env, recipe.id, "")
            .unwrap()
            .replace("{{colors.primary.default.hex}}", "#987654");
        files::write(&path(&env, recipe.id), &custom).unwrap();
        manager::set_with(&env, &config, recipe.id, true, &palette("#123456"), true).unwrap();
        let status = manager::list_with(&env, &config)
            .apps
            .into_iter()
            .find(|app| app.id == recipe.id)
            .unwrap();
        assert!(status.customized);
        assert!(
            files::read(std::path::Path::new(&status.output_path))
                .unwrap()
                .unwrap()
                .contains("#987654"),
            "{}",
            recipe.id
        );
        manager::apply_with(&env, &config, &palette("#abcdef"), true);
        assert!(
            files::read(std::path::Path::new(&status.output_path))
                .unwrap()
                .unwrap()
                .contains("#987654")
        );
        edit_template(&env, recipe.id, true).unwrap();
        manager::apply_with(&env, &config, &palette("#abcdef"), true);
        assert!(
            !files::read(std::path::Path::new(&status.output_path))
                .unwrap()
                .unwrap()
                .contains("#987654")
        );
    }
    assert!(env.receipts.join("backups").read_dir().unwrap().count() >= catalogue::RECIPES.len());
}

#[test]
fn all_recipes_disconnect_without_writes_and_reconnect_after_output_and_import_edits() {
    let (_root, env, config) = fixture();
    for recipe in &catalogue::RECIPES {
        let config_path = env.config.join(recipe.config);
        files::write(&config_path, "").unwrap();
        manager::set_with(&env, &config, recipe.id, true, &palette("#123456"), true).unwrap();
        let status = manager::list_with(&env, &config)
            .apps
            .into_iter()
            .find(|app| app.id == recipe.id)
            .unwrap();
        let output = PathBuf::from(&status.output_path);
        files::write(&output, "my edited colours").unwrap();
        files::write(&config_path, "# my config without a theme import\n").unwrap();
        manager::customize_with(&env, &config, recipe.id, "disconnect", &Value::Null, true)
            .unwrap();
        manager::apply_with(&env, &config, &palette("#abcdef"), true);
        assert_eq!(files::read(&output).unwrap().unwrap(), "my edited colours");
        assert_eq!(
            files::read(&config_path).unwrap().unwrap(),
            "# my config without a theme import\n"
        );
        let status = manager::list_with(&env, &config)
            .apps
            .into_iter()
            .find(|app| app.id == recipe.id)
            .unwrap();
        assert_eq!(status.state, "disconnected");
        assert!(status.can_reconnect && !status.enabled);
        manager::customize_with(&env, &config, recipe.id, "reconnect", &palette("#abcdef"), true)
            .unwrap();
        assert!(files::read(&output).unwrap().unwrap().contains("#abcdef"));
        manager::set_with(&env, &config, recipe.id, false, &Value::Null, true).unwrap();
        assert_eq!(
            files::read(&config_path).unwrap().unwrap(),
            "# my config without a theme import\n"
        );
        let backups = env
            .receipts
            .join("backups")
            .read_dir()
            .unwrap()
            .map(|p| std::fs::read_to_string(p.unwrap().path()).unwrap())
            .collect::<Vec<_>>();
        assert!(backups.iter().any(|text| text.contains("my edited colours")));
    }
}

#[test]
fn invalid_template_preserves_last_good_output_and_failed_reconnect_stays_disconnected() {
    let (_root, env, config) = fixture();
    manager::set_with(&env, &config, "kitty", true, &palette("#123456"), true).unwrap();
    let output = env.config.join("kitty/skwd-colors.conf");
    let before = files::read(&output).unwrap();
    files::write(&path(&env, "kitty"), "foreground {{colors.typo.default.hex}}\n").unwrap();
    assert!(manager::set_with(&env, &config, "kitty", true, &palette("#abcdef"), true).is_err());
    assert_eq!(files::read(&output).unwrap(), before);
    assert!(
        manager::customize_with(&env, &config, "kitty", "reconnect", &palette("#abcdef"), true)
            .is_err()
    );
    assert!(disconnected(&env, "kitty"));
    assert_eq!(files::read(&output).unwrap(), before);
    manager::customize_with(&env, &config, "kitty", "reset-template", &Value::Null, true).unwrap();
    manager::customize_with(&env, &config, "kitty", "reconnect", &palette("#abcdef"), true)
        .unwrap();
    assert!(!disconnected(&env, "kitty"));
}

#[test]
fn niri_reconnection_validates_the_new_include_before_replacing_a_broken_file() {
    use std::os::unix::fs::PermissionsExt;
    let (_root, mut env, config) = fixture();
    manager::set_with(&env, &config, "niri", true, &palette("#123456"), true).unwrap();
    let output = env.config.join("niri/skwd-colors.kdl");
    files::write(&output, "invalid old colours").unwrap();
    let tool = env.search[0].join("niri");
    std::fs::write(
        &tool,
        r#"#!/usr/bin/python3
import re,sys
from pathlib import Path
p=Path(sys.argv[3]);text=p.read_text()
for name in re.findall(r'include "([^"]+)"', text):
    text += (p.parent/name).read_text()
sys.exit(1 if 'invalid old colours' in text else 0)
"#,
    )
    .unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o700)).unwrap();
    env.reload = true;
    manager::customize_with(&env, &config, "niri", "reconnect", &palette("#abcdef"), true).unwrap();
    assert!(files::read(&output).unwrap().unwrap().contains("#abcdef"));
    assert!(
        files::read(&env.config.join("niri/config.kdl"))
            .unwrap()
            .unwrap()
            .contains("include \"skwd-colors.kdl\"")
    );
}
