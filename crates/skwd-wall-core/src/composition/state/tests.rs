use super::WallState;

#[test]
fn state_composes_independently_owned_core_services() {
    let state = WallState::test_new(serde_json::json!({}));

    let config = state.config_store();
    let database = state.database();
    let renderers = state.renderer_supervisor();
    let apply = state.apply_runtime();

    assert_eq!(config.read().display().fill_mode(), "fill");
    assert_eq!(database.with_connection(crate::db::item_count).unwrap(), 0);
    assert_eq!(renderers.wallpaper_count(), 0);
    assert_eq!(apply.generation(), 0);
}
