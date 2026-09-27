use super::super::tests::{fixture, palette as fixture_palette};
use super::*;

#[test]
fn include_preserves_system_defaults_and_trailing_section() {
    let (root, mut env, _) = fixture();
    let system = root.path().join("system/foot/foot.ini");
    files::write(&system, "font=monospace:size=15\n").unwrap();
    env.config_dirs.push(root.path().join("system"));
    let output = env.config.join("foot/skwd-colors.ini");
    let (_, added) = patch(&env, None, &output).unwrap();
    assert!(added.contains(&format!("include={}\n", system.display())));
    assert!(added.contains(&format!("include={}\n", output.display())));
    let (_, added) =
        patch(&env, Some("[key-bindings]\nclipboard-copy=Control+c"), &output).unwrap();
    assert!(added.ends_with("[key-bindings]\n# End Skwd app theme\n"));
    assert!(!added.contains(&system.display().to_string()));
    assert!(patch(&env, None, &env.config.join("bad\ninclude")).is_err());
}

#[test]
fn palette_only_emits_complete_equal_valid_colour_sections() {
    let template = include_str!("../../../../../../data/app-themes/foot.ini");
    let text = crate::static_templates::render_palette(template, &fixture_palette("#123456"), true);
    let commands = palette(&text).unwrap();
    let commands = std::str::from_utf8(&commands).unwrap();
    assert!(commands.contains("\x1b]10;#123456\x1b\\"));
    assert!(commands.contains("\x1b]4;15;#123456\x1b\\"));
    assert!(!commands.contains("\x1b]12;"));
    assert!(palette(&text.replacen("foreground=123456", "foreground=not-a-colour", 1)).is_none());
    assert!(palette(&text.replace("foreground=123456", "foreground=bad")).is_none());
    assert!(palette("[colors-dark]\nbackground=123456\n").is_none());
}

#[test]
fn live_discovery_excludes_special_and_overridden_sessions() {
    for args in [
        b"foot\0".as_slice(),
        b"foot\0-D\0/home/user\0-L\0-H\0-m\0-F\0",
        b"foot\0--title\0A window\0--app-id=test\0sh\0-c\0sleep 10\0",
        b"foot\0--\0command\0--config\0ignored\0",
    ] {
        assert!(ordinary(args), "{args:?}");
    }
    for args in [
        b"foot\0--config=/other\0".as_slice(),
        b"foot\0-c/other\0",
        b"foot\0--title\0A window\0-c\0/other\0",
        b"foot\0--override=colors-dark.background=000000\0",
        b"foot\0-ofoo=bar\0",
        b"foot\0--server=/socket\0",
        b"foot\0-s/socket\0",
        b"foot\0--pty=/dev/pts/8\0",
        b"foot\0--font\0",
        b"foot\0--unknown\0",
    ] {
        assert!(!ordinary(args), "{args:?}");
    }
}

#[test]
fn only_matching_foot_processes_and_master_descriptors_are_selected() {
    let (root, env, _) = fixture();
    let proc = root.path().join("proc");
    let process = proc.join("123");
    files::write(&process.join("cmdline"), "foot\0--title\0Example\0sh\0").unwrap();
    files::write(&process.join("environ"), &format!("XDG_CONFIG_HOME={}\0", env.config.display()))
        .unwrap();
    std::os::unix::fs::symlink(env.executable("foot").unwrap(), process.join("exe")).unwrap();
    files::write(&process.join("fdinfo/5"), "pos:\t0\ntty-index:\t8\n").unwrap();
    files::write(&process.join("fdinfo/6"), "pos:\t0\n").unwrap();
    assert_eq!(terminals(&env, &proc), vec![process.join("fdinfo/5")]);
    files::write(&process.join("cmdline"), "foot\0--server\0").unwrap();
    assert!(terminals(&env, &proc).is_empty());
    files::write(&process.join("cmdline"), "foot\0").unwrap();
    files::write(&process.join("environ"), "XDG_CONFIG_HOME=/other\0").unwrap();
    assert!(terminals(&env, &proc).is_empty());
    files::write(&process.join("environ"), &format!("XDG_CONFIG_HOME={}\0", env.config.display()))
        .unwrap();
    std::fs::remove_file(process.join("exe")).unwrap();
    std::os::unix::fs::symlink(env.executable("kitty").unwrap(), process.join("exe")).unwrap();
    assert!(terminals(&env, &proc).is_empty());
}

#[test]
fn reconnect_recreates_removed_include_and_disable_restores_config() {
    let (_root, env, config) = fixture();
    let colors = fixture_palette("#123456");
    let path = env.config.join("foot/foot.ini");
    let original = "font=monospace:size=15\n";
    files::write(&path, original).unwrap();
    super::super::manager::set_with(&env, &config, "foot", true, &colors, true).unwrap();
    super::super::manager::customize_with(&env, &config, "foot", "disconnect", &colors, true)
        .unwrap();
    files::write(&path, original).unwrap();
    super::super::manager::customize_with(&env, &config, "foot", "reconnect", &colors, true)
        .unwrap();
    assert!(files::read(&path).unwrap().unwrap().contains("skwd-colors.ini"));
    super::super::manager::set_with(&env, &config, "foot", false, &colors, true).unwrap();
    assert_eq!(files::read(&path).unwrap().as_deref(), Some(original));
}

#[test]
fn reconnect_does_not_leave_a_duplicate_owned_include() {
    let (_root, env, config) = fixture();
    let colors = fixture_palette("#123456");
    let path = env.config.join("foot/foot.ini");
    super::super::manager::set_with(&env, &config, "foot", true, &colors, true).unwrap();
    let changed = files::read(&path)
        .unwrap()
        .unwrap()
        .replace("include=", "include = ")
        .replace("# End Skwd app theme", "# user edit\n# End Skwd app theme");
    files::write(&path, &changed).unwrap();
    super::super::manager::customize_with(&env, &config, "foot", "reconnect", &colors, true)
        .unwrap();
    assert_eq!(files::read(&path).unwrap().unwrap().matches("skwd-colors.ini").count(), 1);
    super::super::manager::set_with(&env, &config, "foot", false, &colors, true).unwrap();
    let restored = files::read(&path).unwrap().unwrap();
    assert!(!restored.contains("skwd-colors.ini"));
    assert!(restored.contains("# user edit"));
}
