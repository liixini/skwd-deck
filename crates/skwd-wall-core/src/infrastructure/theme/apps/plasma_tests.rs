use super::*;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;

fn fixture() -> (tempfile::TempDir, Environment, crate::config::Config) {
    let root = tempfile::tempdir().unwrap();
    let env = Environment {
        home: root.path().join("home"),
        config: root.path().join("config"),
        data: root.path().join("data"),
        data_dirs: vec![],
        receipts: root.path().join("state"),
        config_dirs: Vec::new(),
        search: vec![root.path().join("bin")],
        reload: true,
    };
    let tool = env.search[0].join("plasma-apply-colorscheme");
    std::fs::create_dir_all(&env.search[0]).unwrap();
    std::fs::write(&tool, r"#!/usr/bin/python3
import configparser,os,sys
from pathlib import Path
root=Path(os.environ['XDG_CONFIG_HOME'])
if (root/'fail').exists():sys.exit(1)
path=root/'kdeglobals';c=configparser.ConfigParser();c.optionxform=str
if path.exists():c.read(path)
if not c.has_section('General'):c.add_section('General')
c['General']['ColorScheme']=sys.argv[1]
s=configparser.ConfigParser();s.optionxform=str;s.read(Path(os.environ['XDG_DATA_HOME'])/'color-schemes'/(sys.argv[1]+'.colors'))
for section in s.sections():
 if section.startswith('Colors:'):
  if not c.has_section(section):c.add_section(section)
  c[section].update(s[section])
with path.open('w') as f:c.write(f,space_around_delimiters=False)
").unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o700)).unwrap();
    files::write(
        &env.data.join("color-schemes/BreezeDark.colors"),
        "[General]\nName=Breeze Dark\n[Colors:Window]\nBackgroundNormal=1,2,3\n",
    )
    .unwrap();
    files::write(&env.config.join("kdeglobals"), "[General]\nColorScheme=BreezeDark\nfont=Original\n[Colors:Window]\nBackgroundNormal=1,2,3\n").unwrap();
    (root, env, crate::config::Config::from_root(json!({})))
}

fn palette(color: &str) -> Value {
    let mut value = json!({});
    for key in super::super::super::profiles::ROLE_KEYS {
        value[key] = json!(color);
    }
    value
}

#[test]
fn plasma_refreshes_same_scheme_with_alternate_name_and_restores_prior_selection() {
    let (_root, env, config) = fixture();
    assert!(inspect(&env, &config).can_enable);
    set(&env, &config, true, &palette("#123456"), true).unwrap();
    assert_eq!(selected(&env).unwrap(), NAMES[0]);
    assert_eq!(inspect(&env, &config).state, "applied");
    set(&env, &config, true, &Value::Null, true).unwrap();
    assert_eq!(selected(&env).unwrap(), NAMES[1]);
    let path = env.config.join("kdeglobals");
    let text = files::read(&path).unwrap().unwrap().replace("font=Original", "font=Changed");
    files::write(&path, &text).unwrap();
    update(&env, &config, &palette("#abcdef"), true).unwrap();
    assert_eq!(selected(&env).unwrap(), NAMES[0]);
    assert!(files::read(&path).unwrap().unwrap().contains("171,205,239"));
    set(&env, &config, false, &Value::Null, true).unwrap();
    assert_eq!(selected(&env).unwrap(), "BreezeDark");
    let text = files::read(&path).unwrap().unwrap();
    assert!(text.contains("font=Changed") && text.contains("BackgroundNormal=1,2,3"));
    for name in NAMES {
        assert!(!env.data.join(format!("color-schemes/{name}.colors")).exists());
    }
}

#[test]
fn failed_plasma_refresh_remains_undoable_and_does_not_claim_success() {
    let (_root, env, config) = fixture();
    set(&env, &config, true, &palette("#123456"), true).unwrap();
    files::write(&env.config.join("fail"), "").unwrap();
    assert!(update(&env, &config, &palette("#abcdef"), true).is_err());
    let status = inspect(&env, &config);
    assert_eq!(status.state, "interrupted");
    assert!(status.can_disable);
    std::fs::remove_file(env.config.join("fail")).unwrap();
    set(&env, &config, false, &Value::Null, true).unwrap();
    assert_eq!(selected(&env).unwrap(), "BreezeDark");
}

#[test]
fn plasma_preserves_external_output_edits_and_refuses_to_replace_another_selection() {
    let (_root, env, config) = fixture();
    set(&env, &config, true, &palette("#123456"), true).unwrap();
    let path = env.data.join("color-schemes/SkwdManaged.colors");
    files::write(&path, "external edit").unwrap();
    assert!(update(&env, &config, &palette("#abcdef"), true).is_err());
    set(&env, &config, false, &Value::Null, true).unwrap();
    assert_eq!(files::read(&path).unwrap().unwrap(), "external edit");
    std::fs::remove_file(&path).unwrap();
    set(&env, &config, true, &palette("#123456"), true).unwrap();
    files::write(&env.config.join("kdeglobals"), "[General]\nColorScheme=BreezeDark\n").unwrap();
    assert!(!inspect(&env, &config).can_disable);
    assert!(set(&env, &config, false, &Value::Null, true).is_err());
}

#[test]
fn existing_kde_template_offers_migration_and_missing_prior_scheme_blocks_setup() {
    let (_root, env, _) = fixture();
    let config = crate::config::Config::from_root(
        json!({"integrations":[{"name":"kde","template":"kde-colors.colors","output":"old.colors"}]}),
    );
    let status = inspect(&env, &config);
    assert_eq!(status.state, "conflict");
    assert!(status.can_adopt);
    std::fs::remove_file(env.data.join("color-schemes/BreezeDark.colors")).unwrap();
    let config = crate::config::Config::from_root(json!({}));
    assert!(!inspect(&env, &config).can_enable);
    assert!(set(&env, &config, true, &palette("#123456"), true).is_err());
}

#[test]
fn failed_refresh_recovers_automatically_with_the_latest_palette() {
    let (_root, env, config) = fixture();
    set(&env, &config, true, &palette("#123456"), true).unwrap();
    files::write(&env.config.join("fail"), "").unwrap();
    assert!(update(&env, &config, &palette("#abcdef"), true).is_err());
    assert_eq!(inspect(&env, &config).state, "interrupted");
    std::fs::remove_file(env.config.join("fail")).unwrap();
    update(&env, &config, &palette("#fedcba"), true).unwrap();
    assert_eq!(inspect(&env, &config).state, "applied");
    assert!(files::read(&env.config.join("kdeglobals")).unwrap().unwrap().contains("254,220,186"));
    set(&env, &config, false, &Value::Null, true).unwrap();
    assert_eq!(selected(&env).unwrap(), "BreezeDark");
}

#[test]
fn initial_apply_failure_recovers_and_pending_output_edits_are_preserved() {
    let (_root, env, config) = fixture();
    files::write(&env.config.join("fail"), "").unwrap();
    assert!(set(&env, &config, true, &palette("#123456"), true).is_err());
    std::fs::remove_file(env.config.join("fail")).unwrap();
    let path = env.data.join("color-schemes/SkwdManaged.colors");
    files::write(&path, "user edit").unwrap();
    assert!(update(&env, &config, &palette("#abcdef"), true).is_err());
    assert_eq!(selected(&env).unwrap(), "BreezeDark");
    assert_eq!(files::read(&path).unwrap().unwrap(), "user edit");
    let pending = load(&env).unwrap().unwrap().pending_text.unwrap();
    files::write(&path, &pending).unwrap();
    update(&env, &config, &palette("#abcdef"), true).unwrap();
    assert_eq!(inspect(&env, &config).state, "applied");
}

#[test]
fn interrupted_undo_retries_restoration_without_reenabling_the_theme() {
    let (_root, env, config) = fixture();
    set(&env, &config, true, &palette("#123456"), true).unwrap();
    files::write(&env.config.join("fail"), "").unwrap();
    assert!(set(&env, &config, false, &Value::Null, true).is_err());
    std::fs::remove_file(env.config.join("fail")).unwrap();
    update(&env, &config, &palette("#abcdef"), true).unwrap();
    assert_eq!(selected(&env).unwrap(), "BreezeDark");
    assert!(!inspect(&env, &config).enabled);
    assert!(!env.data.join("color-schemes/SkwdManaged.colors").exists());
}

#[test]
fn pending_refresh_does_not_replace_an_external_selection() {
    let (_root, env, config) = fixture();
    set(&env, &config, true, &palette("#123456"), true).unwrap();
    files::write(&env.config.join("fail"), "").unwrap();
    assert!(update(&env, &config, &palette("#abcdef"), true).is_err());
    std::fs::remove_file(env.config.join("fail")).unwrap();
    files::write(&env.config.join("kdeglobals"), "[General]\nColorScheme=UserTheme\n").unwrap();
    assert!(update(&env, &config, &palette("#fedcba"), true).is_err());
    assert_eq!(selected(&env).unwrap(), "UserTheme");
}

#[test]
fn legacy_pending_refresh_and_interrupted_output_write_can_recover() {
    let (_root, env, config) = fixture();
    set(&env, &config, true, &palette("#123456"), true).unwrap();
    let mut receipt = load(&env).unwrap().unwrap();
    receipt.pending = true;
    save(&env, &receipt).unwrap();
    update(&env, &config, &palette("#abcdef"), true).unwrap();
    let mut receipt = load(&env).unwrap().unwrap();
    receipt.pending = true;
    receipt.pending_text =
        Some(manager::rendered(&env, &KDE_RECIPE, &palette("#fedcba"), true).unwrap());
    save(&env, &receipt).unwrap();
    update(&env, &config, &palette("#fedcba"), true).unwrap();
    assert_eq!(inspect(&env, &config).state, "applied");
}

#[test]
fn custom_kde_mappings_survive_refresh_and_disconnect_preserves_manual_scheme() {
    let (_root, env, config) = fixture();
    let custom = env.config.join("skwd-wall-v2/app-themes/kde.template");
    let template = KDE_RECIPE.template.replace("colors.primary.", "colors.tertiary.");
    files::write(&custom, &template).unwrap();
    let mut colors = palette("#123456");
    colors["tertiary"] = json!("#abcdef");
    manager::set_with(&env, &config, "kde", true, &colors, true).unwrap();
    assert!(
        files::read(&env.data.join("color-schemes/SkwdManaged.colors"))
            .unwrap()
            .unwrap()
            .contains("171,205,239")
    );
    files::write(
        &env.config.join("kdeglobals"),
        "[General]\nColorScheme=BreezeDark\nfont=Manual\n",
    )
    .unwrap();
    let before = files::read(&env.config.join("kdeglobals")).unwrap();
    manager::customize_with(&env, &config, "kde", "disconnect", &Value::Null, true).unwrap();
    manager::apply_with(&env, &config, &palette("#fedcba"), true);
    assert_eq!(files::read(&env.config.join("kdeglobals")).unwrap(), before);
    manager::customize_with(&env, &config, "kde", "reconnect", &colors, true).unwrap();
    assert_eq!(inspect(&env, &config).state, "applied");
    manager::set_with(&env, &config, "kde", false, &Value::Null, true).unwrap();
    assert_eq!(selected(&env).unwrap(), "BreezeDark");
    assert!(files::read(&env.config.join("kdeglobals")).unwrap().unwrap().contains("font=Manual"));
}
