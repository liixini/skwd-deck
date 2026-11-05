use anyhow::Result;
use serde_json::{Value, json};
use skwd_config::keys::drawing as keys;
use wall_proto::ev;

use crate::backend::events::EventPublisher;
use crate::composition::context::Ctx;

pub(super) fn save(ctx: &Ctx, key: &str, enabled: bool) -> Result<()> {
    super::update_config(|drawing| {
        if !drawing[keys::WALLPAPERS].is_object() {
            drawing[keys::WALLPAPERS] = json!({});
        }
        drawing[keys::WALLPAPERS][key] = Value::Bool(enabled);
    })?;
    changed(ctx);
    Ok(())
}

pub(super) fn changed(ctx: &Ctx) {
    ctx.state.reload_config();
    ctx.events.publish(
        ev::CONFIG_CHANGED,
        serde_json::json!(ev::ConfigChanged { library_changed: false }),
    );
}
