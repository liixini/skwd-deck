use super::*;

#[test]
fn legacy_assignment_metadata() {
    let assignment = super::super::Assignment {
        outputs: vec!["DP-1".into()],
        source: super::super::Source::static_file("/wall/a.png"),
        fill_mode: paper_control::FillMode::default(),
        background: paper_control::Background::default(),
        mute: true,
        volume: 42,
        layer: super::super::Layer::Background,
        transition: None,
    };
    let entry = serde_json::json!({"assignment":assignment});
    assert_eq!(
        legacy_wallpaper(&entry).unwrap(),
        crate::audio::entry("static", "/wall/a.png", "", true, 42)
    );
}

#[test]
fn script_scopes_activity() {
    let payload = serde_json::json!({"DP-1":{"assignment":{}}});
    let script = super::super::script(&payload, "a\"b", true, &["DP-1".into()]).unwrap();
    assert!(script.contains("desktopsForActivity(target)"));
    assert!(script.contains("activities().indexOf(target)<0"));
    assert!(script.contains(r#"var target="a\"b""#));
    assert!(script.contains("merged[output]=a[output]"));
    assert!(script.contains("screenForConnector(output)"));
    assert!(script.contains("before.indexOf(desktop.id)<0"));
    assert!(script.contains("spare.screen=screen"));
    assert!(!script.contains("setCurrentActivity"));
}
