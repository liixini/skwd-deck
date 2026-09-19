use super::*;
use serde_json::json;

fn input(index: u32, corked: bool, binary: &str, pid: u32) -> Value {
    json!({
        "index": index,
        "corked": corked,
        "mute": false,
        "properties": {
            "application.name": binary,
            "application.process.binary": binary,
            "application.process.id": pid.to_string(),
        }
    })
}

#[test]
fn corked_and_own_streams_do_not_count() {
    let own = HashSet::from([4242]);
    assert!(!other_stream_playing(&json!([]), &own));
    assert!(!other_stream_playing(&json!([input(1, true, "firefox", 100)]), &own));
    assert!(!other_stream_playing(&json!([input(2, false, "skwd-wall-vk", 100)]), &own));
    assert!(!other_stream_playing(&json!([input(3, false, "ALSA plug-in", 4242)]), &own));
    assert!(other_stream_playing(&json!([input(4, false, "firefox", 100)]), &own));
    assert!(other_stream_playing(
        &json!([
            input(5, true, "mpv", 7),
            input(6, false, "skwd-wall-vk", 4242),
            input(7, false, "spotify", 9)
        ]),
        &own
    ));
}

#[test]
fn renderer_streams_through_the_alsa_plugin_do_not_count() {
    let own = HashSet::new();
    let alsa = json!({
        "index": 8,
        "corked": false,
        "mute": false,
        "properties": {
            "application.name": "PipeWire ALSA [skwd-wall-vk]",
            "node.name": "alsa_playback.skwd-wall-vk",
            "media.name": "ALSA Playback"
        }
    });
    assert!(!other_stream_playing(&json!([alsa]), &own));
    let other = json!({
        "index": 9,
        "corked": false,
        "mute": false,
        "properties": {"application.name": "PipeWire ALSA [mpv]", "node.name": "alsa_playback.mpv"}
    });
    assert!(other_stream_playing(&json!([other]), &own));
}

#[test]
fn captured_mpv_media_title_is_not_renderer_identity() {
    let inputs = serde_json::from_str(include_str!("tests/mpv-skwd-title.json")).unwrap();
    assert!(other_stream_playing(&inputs, &HashSet::new()));
}

#[test]
fn captured_paper_alsa_identity_does_not_need_a_process_id() {
    let inputs = serde_json::from_str(include_str!("tests/paper-alsa.json")).unwrap();
    assert!(!other_stream_playing(&inputs, &HashSet::new()));
}

#[test]
fn renderer_names_must_identify_the_renderer() {
    for (key, value) in [
        ("application.name", "Watching skwd-wall-vk"),
        ("node.name", "mpv.skwd-wall-vk-demo"),
        ("media.name", "skwd-wall-vk"),
        ("application.process.binary", "skwd-demo-player"),
    ] {
        let inputs = json!([{"corked": false, "properties": {key: value}}]);
        assert!(other_stream_playing(&inputs, &HashSet::new()), "{key}: {value}");
    }
}

#[test]
fn muted_streams_and_missing_fields_do_not_count() {
    let own = HashSet::new();
    let mut muted = input(1, false, "firefox", 100);
    muted["mute"] = json!(true);
    assert!(!other_stream_playing(&json!([muted]), &own));
    assert!(!other_stream_playing(&json!([{"index": 2}]), &own));
    assert!(other_stream_playing(&json!([{"index": 3, "corked": false}]), &own));
    assert!(!other_stream_playing(&json!({"index": 3, "corked": false}), &own));
}

#[test]
fn only_stream_events_trigger_observation() {
    assert!(is_stream_event("Event 'change' on sink-input #1911"));
    assert!(is_stream_event("Event 'new' on sink-input #12"));
    assert!(is_stream_event("Event 'remove' on sink-input #12"));
    assert!(!is_stream_event("Event 'change' on sink #67"));
    assert!(!is_stream_event("Event 'change' on client #765"));
    assert!(!is_stream_event("Event 'change' on source-output #3"));
}
