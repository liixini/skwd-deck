use crate::infrastructure::stats::Stats;
use wall_proto::Response;

pub(crate) fn fail(stats: &Stats, id: u64, error: impl std::fmt::Display) -> Response {
    stats.error();
    Response::err(id, -1, error.to_string())
}

pub(crate) fn fail_msg(stats: &Stats, id: u64, code: i32, message: impl Into<String>) -> Response {
    stats.error();
    Response::err(id, code, message.into())
}

pub(super) fn classify_apply_error(detail: &str) -> &'static str {
    let detail = detail.to_lowercase();
    if detail.contains("renderer_unavailable")
        || detail.contains("media capability requires skwd-wall")
    {
        "renderer_unavailable"
    } else if detail.contains("missing path") || detail.contains("missing we_id") {
        "bad_request"
    } else if detail.contains("no such file")
        || detail.contains("does not exist")
        || detail.contains("not found")
    {
        "file_missing"
    } else if detail.contains("decode") || detail.contains("ffmpeg") || detail.contains("image") {
        "decode_failed"
    } else if detail.contains("spawn")
        || detail.contains("renderer")
        || detail.contains("os error 2")
    {
        "renderer_spawn_failed"
    } else if detail.contains("output") || detail.contains("monitor") {
        "no_outputs"
    } else {
        "apply_failed"
    }
}
