use super::*;

#[test]
fn clock_catches_stale() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({
        "theme": {"policy": "wallpaper", "engine": wall_proto::STUDIO_ENGINE, "studioMode": "by-time"}
    }));
    let (state, _, _) = crate::testenv::harness();
    state.theme().set_source("/clock.jpg");
    let active = current(&state).expect("by-time mode is active");
    assert_eq!(active.source, "/clock.jpg");
    assert!(!active.applied);
    assert!(active.wait > Duration::ZERO);
    state.theme().set_applied_theme(serde_json::json!({
        "source": "/clock.jpg", "dark": active.dark
    }));
    assert!(current(&state).expect("active").applied);
    state.theme().set_applied_theme(serde_json::json!({
        "source": "/other.jpg", "dark": active.dark
    }));
    assert!(!current(&state).expect("active").applied);
}
