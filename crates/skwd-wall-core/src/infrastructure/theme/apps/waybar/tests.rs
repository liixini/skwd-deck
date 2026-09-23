use super::super::tests::{fixture, palette};
use super::*;
use serde_json::json;

const ORIGINAL: &str =
    "@import \"modules.css\";\nwindow#waybar { font-size: 13px; background: #123456; }\n";

#[test]
fn variants_import_overlay_and_restore_unrelated_edits() {
    let (_root, env, config) = fixture();
    for name in ["style.css", "style-light.css", "style-dark.css"] {
        files::write(&env.config.join("waybar").join(name), ORIGINAL).unwrap();
    }
    set(&env, &config, true, &palette("#abcdef"), true).unwrap();
    let status = inspect(&env, &config);
    assert!(status.enabled && status.can_disable);
    assert_eq!(status.config_path.lines().count(), 3);
    let before = files::read(&output_path(&env)).unwrap().unwrap();
    assert!(before.contains("#abcdef") && !before.contains("{{"));
    let changed = env.config.join("waybar/style.css");
    let css = files::read(&changed).unwrap().unwrap();
    files::write(&changed, &format!("{css}#clock {{ padding: 7px; }}\n")).unwrap();
    update(&env, &config, &palette("#fedcba"), false).unwrap();
    assert_ne!(before, files::read(&output_path(&env)).unwrap().unwrap());
    assert_eq!(
        files::read(&changed).unwrap().unwrap(),
        format!("{css}#clock {{ padding: 7px; }}\n")
    );
    set(&env, &config, false, &Value::Null, true).unwrap();
    assert_eq!(
        files::read(&changed).unwrap().unwrap(),
        format!("{ORIGINAL}#clock {{ padding: 7px; }}\n")
    );
    assert_eq!(files::read(&env.config.join("waybar/style-light.css")).unwrap().unwrap(), ORIGINAL);
    assert!(!output_path(&env).exists());
}

#[test]
fn system_default_gets_a_reversible_user_import_wrapper() {
    let (root, mut env, config) = fixture();
    let directory = root.path().join("system");
    let source = directory.join("waybar/style.css");
    files::write(&source, ORIGINAL).unwrap();
    env.config_dirs.push(directory);
    assert!(inspect(&env, &config).can_enable);
    set(&env, &config, true, &palette("#abcdef"), true).unwrap();
    let path = env.config.join("waybar/style.css");
    let text = files::read(&path).unwrap().unwrap();
    assert!(text.contains(&paths::css_path(&source)));
    assert_eq!(files::read(&source).unwrap().unwrap(), ORIGINAL);
    set(&env, &config, false, &Value::Null, true).unwrap();
    assert!(!path.exists());
    assert_eq!(files::read(&source).unwrap().unwrap(), ORIGINAL);
}

#[test]
fn symlinked_stylesheet_keeps_its_link_and_targets_the_user_file() {
    let (root, env, config) = fixture();
    let target = root.path().join("dotfiles/bar.css");
    files::write(&target, ORIGINAL).unwrap();
    std::fs::create_dir_all(env.config.join("waybar")).unwrap();
    let link = env.config.join("waybar/style.css");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    set(&env, &config, true, &palette("#abcdef"), true).unwrap();
    assert!(link.is_symlink());
    assert!(files::read(&target).unwrap().unwrap().contains("skwd-theme.css"));
    set(&env, &config, false, &Value::Null, true).unwrap();
    assert_eq!(files::read(&target).unwrap().unwrap(), ORIGINAL);
    assert!(link.is_symlink());
}

#[test]
fn missing_import_can_be_disconnected_and_reenabled() {
    let (_root, env, config) = fixture();
    let path = env.config.join("waybar/style.css");
    files::write(&path, ORIGINAL).unwrap();
    set(&env, &config, true, &palette("#abcdef"), true).unwrap();
    files::write(&path, ORIGINAL).unwrap();
    assert!(update(&env, &config, &palette("#fedcba"), false).is_err());
    assert!(inspect(&env, &config).can_disable);
    set(&env, &config, false, &Value::Null, true).unwrap();
    assert_eq!(files::read(&path).unwrap().unwrap(), ORIGINAL);
    set(&env, &config, true, &palette("#fedcba"), false).unwrap();
    assert!(inspect(&env, &config).enabled);
}

#[test]
fn modified_generated_file_is_preserved_and_missing_file_does_not_block_disconnect() {
    let (_root, env, config) = fixture();
    files::write(&env.config.join("waybar/style.css"), ORIGINAL).unwrap();
    set(&env, &config, true, &palette("#abcdef"), true).unwrap();
    files::write(&output_path(&env), "user edit").unwrap();
    assert_eq!(inspect(&env, &config).state, "changed");
    set(&env, &config, false, &Value::Null, true).unwrap();
    assert_eq!(files::read(&output_path(&env)).unwrap().unwrap(), "user edit");
    assert!(set(&env, &config, true, &palette("#abcdef"), true).is_err());
    std::fs::remove_file(output_path(&env)).unwrap();
    set(&env, &config, true, &palette("#abcdef"), true).unwrap();
    std::fs::remove_file(output_path(&env)).unwrap();
    set(&env, &config, false, &Value::Null, true).unwrap();
}

#[test]
fn custom_output_conflicts_use_destinations_and_protect_enabled_files() {
    let (_root, env, config) = fixture();
    let style = env.config.join("waybar/style.css");
    files::write(&style, ORIGINAL).unwrap();
    for output in
        [output_path(&env), style.clone(), env.config.join("waybar/../waybar/skwd-theme.css")]
    {
        let collision = Config::from_root(
            json!({"integrations":[{"name":"unrelated-name","template":"custom.css","output":output}]}),
        );
        assert_eq!(inspect(&env, &collision).state, "conflict");
        assert!(set(&env, &collision, true, &palette("#abcdef"), true).is_err());
        assert_eq!(files::read(&style).unwrap().unwrap(), ORIGINAL);
    }
    set(&env, &config, true, &palette("#abcdef"), true).unwrap();
    assert!(protects(&env, &style));
    assert!(protects(&env, &output_path(&env)));
    let collision = Config::from_root(
        json!({"integrations":[{"name":"late-output","template":"custom.css","output":output_path(&env)}]}),
    );
    let before = files::read(&output_path(&env)).unwrap();
    assert!(update(&env, &collision, &palette("#fedcba"), false).is_err());
    assert_eq!(files::read(&output_path(&env)).unwrap(), before);
    assert!(inspect(&env, &collision).can_disable);
    set(&env, &collision, false, &Value::Null, true).unwrap();
    assert!(!protects(&env, &style));
}

#[test]
fn active_stylesheet_arguments_accept_short_long_and_relative_paths() {
    for args in [
        vec!["waybar", "-s", "custom.css"],
        vec!["waybar", "--style", "custom.css"],
        vec!["waybar", "--style=custom.css"],
        vec!["waybar", "-scustom.css"],
    ] {
        let args = args.into_iter().map(str::to_string).collect::<Vec<_>>();
        assert_eq!(
            paths::argument(&args, Path::new("/working")).unwrap(),
            Some(PathBuf::from("/working/custom.css"))
        );
    }
    assert!(paths::argument(&["waybar".into(), "--style".into()], Path::new("/")).is_err());
    assert_eq!(paths::argument(&["waybar".into()], Path::new("/")).unwrap(), None);
}

#[test]
fn partial_setup_restores_written_imports_without_losing_untouched_styles() {
    let (_root, env, config) = fixture();
    let path = env.config.join("waybar/style.css");
    files::write(&path, ORIGINAL).unwrap();
    set(&env, &config, true, &palette("#abcdef"), true).unwrap();
    let mut receipt = load(&env).unwrap().unwrap();
    receipt.enabled = false;
    receipt.pending = true;
    save(&env, &receipt).unwrap();
    assert_eq!(inspect(&env, &config).state, "interrupted");
    set(&env, &config, false, &Value::Null, true).unwrap();
    assert_eq!(files::read(&path).unwrap().unwrap(), ORIGINAL);
    assert!(!output_path(&env).exists());
}

#[test]
fn legacy_palette_import_is_migrated_without_losing_user_css() {
    let (_root, env, config) = fixture();
    let path = env.config.join("waybar/style.css");
    let output = env.config.join("waybar/skwd-colors.css");
    let (before, after) = files::patch(recipe(), ORIGINAL).unwrap();
    files::write(
        &path,
        &format!("{ORIGINAL}{after}").replace("/* Skwd app theme */", "/*  Skwd app theme  */"),
    )
    .unwrap();
    files::write(&output, "legacy colours").unwrap();
    files::save(
        &env.receipts.join("waybar.json"),
        &files::Receipt {
            version: 1,
            enabled: true,
            pending: false,
            pending_config: None,
            config: path.clone(),
            output: output.clone(),
            original: Some(ORIGINAL.into()),
            before,
            after,
            rendered: "legacy colours".into(),
            result: "configured".into(),
        },
    )
    .unwrap();
    assert!(inspect(&env, &config).enabled);
    set(&env, &config, true, &palette("#abcdef"), true).unwrap();
    assert!(!output.exists());
    assert!(!files::read(&path).unwrap().unwrap().contains("skwd-colors.css"));
    set(&env, &config, false, &Value::Null, true).unwrap();
    assert_eq!(files::read(&path).unwrap().unwrap(), ORIGINAL);
}

#[test]
fn formatted_imports_remain_connected_and_can_be_removed() {
    let (_root, env, config) = fixture();
    let path = env.config.join("waybar/style.css");
    files::write(&path, ORIGINAL).unwrap();
    set(&env, &config, true, &palette("#abcdef"), true).unwrap();
    let text = files::read(&path).unwrap().unwrap().replace(
        &directive(&output_path(&env)),
        &format!("  @import  url(\"{}\")  ;", paths::css_path(&output_path(&env))),
    );
    files::write(&path, &text).unwrap();
    update(&env, &config, &palette("#fedcba"), false).unwrap();
    set(&env, &config, false, &Value::Null, true).unwrap();
    let restored = files::read(&path).unwrap().unwrap();
    assert_eq!(restored.trim_end(), ORIGINAL.trim_end());
    assert!(!restored.contains("skwd-theme"));
}

#[test]
fn overlay_only_changes_colour_properties() {
    let css = rendered(&palette("#abcdef"), true).unwrap();
    for block in css.split('{').skip(1) {
        let body = block.split('}').next().unwrap();
        for declaration in body.split(';').filter(|text| !text.trim().is_empty()) {
            let (property, _) = declaration.split_once(':').unwrap();
            assert!(
                ["background-color", "color", "border-color"].contains(&property.trim()),
                "{declaration}"
            );
        }
    }
}

#[test]
fn existing_palette_variables_and_separate_custom_outputs_keep_working() {
    let (_root, env, _) = fixture();
    files::write(&env.config.join("waybar/style.css"), "window#waybar { color: @primary; }\n")
        .unwrap();
    let config = Config::from_root(
        json!({"integrations":[{"name":"waybar","template":"waybar.css","output":"old-palette.css"}]}),
    );
    assert!(inspect(&env, &config).can_enable);
    set(&env, &config, true, &palette("#abcdef"), true).unwrap();
    let text = files::read(&output_path(&env)).unwrap().unwrap();
    assert!(
        text.contains("@define-color primary #abcdef;")
            && text.contains("@define-color skwd_primary #abcdef;")
    );
}
