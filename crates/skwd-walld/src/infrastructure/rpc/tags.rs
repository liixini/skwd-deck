use std::sync::Arc;

use serde_json::json;
use skwd_wall_core::{WallState, db};
use wall_proto::{Request, Response};

use super::response::fail;
use crate::infrastructure::stats::Stats;

pub(super) fn update_tags(state: &Arc<WallState>, req: &Request, stats: &Arc<Stats>) -> Response {
    let key = req.str_param("key", "");
    let tags = req.opt_str("tags");
    let Some(tags) = tags else {
        return Response::err(req.id, -32602, "missing tags");
    };
    match state.with_db(|conn| db::update_user_tags(conn, key, tags)) {
        Ok(_) => Response::ok(req.id, json!({"updated": key})),
        Err(err) => fail(stats, req.id, err),
    }
}
