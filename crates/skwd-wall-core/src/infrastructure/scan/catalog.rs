use serde_json::json;

pub(super) struct Row {
    pub(super) key: String,
    pub(super) name: String,
    pub(super) thumb: String,
    pub(super) thumb_sm: String,
    pub(super) mtime: i64,
    pub(super) hue: i64,
    pub(super) sat: i64,
    pub(super) richness: i64,
    pub(super) filesize: i64,
    pub(super) width: i64,
    pub(super) height: i64,
}

pub(super) fn row_item_json(row: &Row) -> serde_json::Value {
    json!({
        "key": row.key,
        "name": row.name,
        "type": wall_proto::kind::STATIC,
        "thumb": row.thumb,
        "thumb_sm": row.thumb_sm,
        "favourite": 0,
        "hue": row.hue,
        "sat": row.sat,
        "tags": serde_json::Value::Null,
        "colors": serde_json::Value::Null,
        "video_file": serde_json::Value::Null,
        "we_id": serde_json::Value::Null,
        "filesize": row.filesize,
        "width": row.width,
        "height": row.height,
        "mtime": row.mtime,
        "richness": row.richness,
        "apply_count": 0,
        "last_applied": 0,
    })
}
