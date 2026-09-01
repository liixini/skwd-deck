#![cfg(test)]

use super::{TEMPLATES, seed};

#[test]
fn seed_once_no_clobber() {
    let dir = tempfile::tempdir().unwrap();
    let written = seed(dir.path());
    assert!(written >= 15, "got {written}");
    assert!(dir.path().join("quickshell-colors.json").is_file());
    assert!(dir.path().join("skwd-aura-colors.conf").is_file());
    let kitty = dir.path().join("kitty.conf");
    std::fs::write(&kitty, "my edited template").unwrap();
    let again = seed(dir.path());
    assert_eq!(again, 0);
    assert_eq!(std::fs::read_to_string(&kitty).unwrap(), "my edited template");
}

#[test]
fn vesktop_editor_roles() {
    let css = TEMPLATES
        .iter()
        .find_map(|(name, content)| (*name == "vesktop.css").then_some(*content))
        .expect("embedded Vesktop template");

    assert!(css.contains("--skwd-editor-surface: rgba(var(--skwd-panel-raised-rgb), 0.92)"));
    assert!(css.contains("--skwd-editor-text: var(--skwd-panel-text)"));
    assert!(css.contains("--chat-background-default: var(--skwd-editor-surface)"));
    assert!(css.contains("div[class*=\"channelTextArea\"] div[class*=\"scrollableContainer\"]"));
    assert!(css.contains("color: var(--skwd-editor-text) !important"));
    assert!(css.contains("color: var(--skwd-editor-placeholder) !important"));
}

#[test]
fn foot_opaque_alpha() {
    let config = TEMPLATES
        .iter()
        .find_map(|(name, content)| (*name == "foot.ini").then_some(*content))
        .expect("embedded foot template");
    assert_eq!(config.lines().filter(|line| *line == "alpha=1.0").count(), 2);
}

#[test]
fn aura_semantic_roles() {
    let palette = TEMPLATES
        .iter()
        .find_map(|(name, content)| (*name == "skwd-aura-colors.conf").then_some(*content))
        .expect("embedded skwd-aura template");
    assert!(palette.contains("ink={{colors.surface.default.hex}}"));
    assert!(palette.contains("paper={{colors.on_surface.default.hex}}"));
    assert!(palette.contains("acid={{colors.primary.default.hex}}"));
}

#[test]
fn btop_roles_complete() {
    let theme = TEMPLATES
        .iter()
        .find_map(|(name, content)| (*name == "btop.theme").then_some(*content))
        .expect("embedded btop template");
    assert!(theme.contains(r#"theme[main_bg]="{{colors.surface.default.hex}}""#));
    assert!(!theme.contains(r#"theme[main_bg]="{{colors.background.default.hex}}""#));
    for role in [
        "main_bg",
        "main_fg",
        "title",
        "hi_fg",
        "selected_bg",
        "selected_fg",
        "inactive_fg",
        "graph_text",
        "meter_bg",
        "proc_misc",
        "cpu_box",
        "mem_box",
        "net_box",
        "proc_box",
        "div_line",
        "temp_start",
        "temp_mid",
        "temp_end",
        "cpu_start",
        "cpu_mid",
        "cpu_end",
        "free_start",
        "free_mid",
        "free_end",
        "cached_start",
        "cached_mid",
        "cached_end",
        "available_start",
        "available_mid",
        "available_end",
        "used_start",
        "used_mid",
        "used_end",
        "download_start",
        "download_mid",
        "download_end",
        "upload_start",
        "upload_mid",
        "upload_end",
        "process_start",
        "process_mid",
        "process_end",
        "proc_pause_bg",
        "proc_follow_bg",
        "proc_banner_bg",
        "proc_banner_fg",
        "followed_bg",
        "followed_fg",
    ] {
        assert!(theme.contains(&format!("theme[{role}]=")), "btop role {role}");
    }

    let document = skwd_wall_core::material::document("#f06e44", true).unwrap();
    let rendered = skwd_wall_core::static_templates::render_doc(theme, &document);
    assert!(!rendered.contains("{{"), "unresolved btop palette token");
    for line in rendered.lines().filter(|line| !line.is_empty()) {
        let value = line.split_once('=').unwrap().1.trim_matches('"');
        assert!(value.starts_with('#') && value.len() == 7, "btop colour {line}");
    }
}

#[test]
fn embedded_set_matches_packaged_dir() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/matugen/templates");
    let mut on_disk: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    on_disk.sort();
    let mut embedded: Vec<String> = TEMPLATES.iter().map(|(name, _)| (*name).to_string()).collect();
    embedded.sort();
    assert_eq!(embedded, on_disk);
}
