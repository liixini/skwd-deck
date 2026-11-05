use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};

use serde_json::json;
use skwd_wall_core::WallState;
use skwd_wall_core::infrastructure::drawing as cache;
use skwd_wall_core::infrastructure::renderers::WidgetSource;
use wall_proto::widgets::Desk;
use wall_proto::{Request, Response, rpc};

use crate::infrastructure::events::EventHub;
use crate::infrastructure::stats::Stats;

use super::{Desktop, FALLBACK, MAX_WIDGETS, palette, placed};

fn clock(height: f64) -> serde_json::Value {
    json!({
        "face": {"kind": "clock", "style": "aperture"},
        "depth": "behind",
        "pin": {"anchor": [0.5, 0.5], "offset": [0.0, 0.0]},
        "height": height,
    })
}

#[test]
fn placed_skips_invalid() {
    let root = json!({"widgets": {"items": [clock(0.3), clock(2.0), {"face": "dial"}]}});
    assert_eq!(placed(&root).len(), 1);
}

#[test]
fn placed_caps_count() {
    let root = json!({"widgets": {"items": vec![clock(0.3); MAX_WIDGETS + 4]}});
    assert_eq!(placed(&root).len(), MAX_WIDGETS);
}

#[test]
fn palette_reads_roles() {
    assert_eq!(palette(None), FALLBACK);
    let colors = json!({"surfaceText": "#dbe3e5", "primary": "#64a6d8", "surface": "#080e16"});
    let read = palette(Some(&colors));
    assert_eq!(
        (read.text, read.accent, read.shade),
        ([219, 227, 229], [100, 166, 216], [8, 14, 22])
    );
    assert_eq!(palette(Some(&json!({"primary": "#64a6d8"}))), FALLBACK);
}

type Harness = (Arc<WallState>, Arc<EventHub>, Arc<Stats>);

fn desktop(state: &Arc<WallState>) -> (Desktop, Receiver<PathBuf>) {
    let (wanted, missing) = channel();
    (Desktop { state: Arc::downgrade(state), outlines: Mutex::default(), wanted }, missing)
}

fn harnessed(config: serde_json::Value) -> Harness {
    crate::testenv::write_config(config);
    crate::testenv::harness()
}

fn call((state, events, stats): &Harness, method: &str, params: serde_json::Value) -> Response {
    let ctx = crate::testenv::context(state, events, stats);
    crate::infrastructure::rpc::dispatch(&ctx, &Request { id: 7, method: method.into(), params })
}

#[test]
fn memo_follows_index() {
    let (_guard, root) = crate::testenv::lock();
    let cache_root = tempfile::tempdir().unwrap();
    let (state, _, _) = harnessed(json!({
        "paths": {"cache": cache_root.path()},
        "widgets": {"items": [clock(0.3)]},
    }));
    let source = root.join("walls/outlined.png");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, b"original").unwrap();
    let (desktop, _missing) = desktop(&state);
    let shown = [source.to_string_lossy().into_owned()];
    assert!(desktop.scene(&shown).outlines.is_empty());
    let artifact = crate::testenv::drawn(&state.config(), &source);
    assert_eq!(desktop.scene(&shown).outlines, [artifact.join(cache::MANIFEST)]);
}

#[test]
fn wants_missing_once() {
    let (_guard, root) = crate::testenv::lock();
    let cache_root = tempfile::tempdir().unwrap();
    let (state, _, _) = harnessed(json!({
        "paths": {"cache": cache_root.path()},
        "widgets": {"items": [clock(0.3)]},
    }));
    let source = root.join("walls/bare.png");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, b"original").unwrap();
    let (desktop, missing) = desktop(&state);
    let shown = [source.to_string_lossy().into_owned()];
    desktop.scene(&shown);
    desktop.scene(&shown);
    assert_eq!(missing.try_iter().collect::<Vec<_>>(), [source]);
}

#[test]
fn front_wants_nothing() {
    let (_guard, root) = crate::testenv::lock();
    let mut front = clock(0.3);
    front["depth"] = json!("front");
    let (state, _, _) = harnessed(json!({"widgets": {"items": [front]}}));
    let source = root.join("walls/front.png");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, b"original").unwrap();
    let (desktop, missing) = desktop(&state);
    desktop.scene(&[source.to_string_lossy().into_owned()]);
    assert_eq!(missing.try_iter().count(), 0);
}

#[test]
fn desk_keeps_outline() {
    let (_guard, _) = crate::testenv::lock();
    let cache_root = tempfile::tempdir().unwrap();
    let harness = harnessed(json!({"paths": {"cache": cache_root.path()}}));
    let (source, _missing) = desktop(&harness.0);
    harness.0.renderers().set_widget_source(Arc::new(source));
    let dir = tempfile::tempdir().unwrap();
    let still = dir.path().join("a.png");
    std::fs::write(&still, b"still").unwrap();
    let artifact = crate::testenv::drawn(&harness.0.config(), &still);
    harness.0.renderers().set_assignment("DP-1", &still.to_string_lossy());
    harness.0.renderers().set_assignment("DP-2", &still.to_string_lossy());
    let response = call(&harness, rpc::WIDGETS_SCENE, json!({}));
    let desk: Desk = serde_json::from_value(response.result.unwrap()).unwrap();
    assert_eq!(desk.scene.outlines, [artifact.join(cache::MANIFEST)]);
}

#[test]
fn edit_replies() {
    let (_guard, _) = crate::testenv::lock();
    let harness = harnessed(json!({}));
    let response = call(&harness, rpc::WIDGETS_EDIT, json!({"active": true}));
    assert_eq!(response.result.unwrap(), json!({"active": true}));
    assert!(!harness.0.renderers().widget_editing());
    assert!(call(&harness, rpc::WIDGETS_EDIT, json!({})).error.is_some());
}

fn editor(
    ctx: &crate::composition::context::Ctx,
    runtime: &tokio::runtime::Runtime,
) -> (std::os::unix::net::UnixStream, tokio::task::JoinHandle<()>) {
    use std::io::{BufRead, BufReader, Write};
    let (client, server) = std::os::unix::net::UnixStream::pair().unwrap();
    let ctx = ctx.clone();
    let task = runtime.spawn(async move {
        server.set_nonblocking(true).unwrap();
        let server = tokio::net::UnixStream::from_std(server).unwrap();
        crate::infrastructure::rpc::handle_conn(server, &ctx).await;
    });
    let mut writer = client.try_clone().unwrap();
    writer
        .write_all(b"{\"method\":\"widgets.edit\",\"id\":1,\"params\":{\"active\":true}}\n")
        .unwrap();
    BufReader::new(client).read_line(&mut String::new()).unwrap();
    (writer, task)
}

#[test]
fn disconnect_stops_editing() {
    let (_guard, _) = crate::testenv::lock();
    let (state, events, stats) = harnessed(json!({}));
    let ctx = crate::testenv::context(&state, &events, &stats);
    let runtime = crate::testenv::runtime();
    let (first, first_task) = editor(&ctx, &runtime);
    let (second, second_task) = editor(&ctx, &runtime);
    assert!(state.renderers().widget_editing());
    drop(first);
    runtime.block_on(first_task).unwrap();
    assert!(state.renderers().widget_editing());
    drop(second);
    runtime.block_on(second_task).unwrap();
    assert!(!state.renderers().widget_editing());
}

#[test]
fn scene_rpc_desk() {
    let (_guard, _) = crate::testenv::lock();
    let harness = harnessed(json!({
        "widgets": {"items": [clock(0.3)]},
        "display": {"fillModes": {"DP-2": "fit"}},
    }));
    let (source, _missing) = desktop(&harness.0);
    harness.0.renderers().set_widget_source(Arc::new(source));
    let dir = tempfile::tempdir().unwrap();
    let still = dir.path().join("a.png");
    std::fs::write(&still, b"still").unwrap();
    let still = still.to_string_lossy().into_owned();
    harness.0.renderers().set_assignment("DP-2,DP-1", &still);
    harness.0.renderers().set_assignment("HDMI-A-1", "/w/b.mp4");
    let response = call(&harness, rpc::WIDGETS_SCENE, json!({}));
    let desk: Desk = serde_json::from_value(response.result.unwrap()).unwrap();
    assert_eq!(desk.scene.widgets.len(), 1);
    assert!(desk.scene.outlines.is_empty());
    let names: Vec<_> = desk.outputs.iter().map(|output| output.name.as_str()).collect();
    assert_eq!(names, ["DP-1", "DP-2"]);
    assert_eq!(desk.outputs[1].fill, "fit");
    assert_eq!(desk.outputs[0].path, still);
}

#[test]
fn palette_reads_cache() {
    let (_guard, _) = crate::testenv::lock();
    let cache_root = tempfile::tempdir().unwrap();
    let (state, _, _) = harnessed(json!({"paths": {"cache": cache_root.path()}}));
    let mut colors: serde_json::Map<String, serde_json::Value> =
        skwd_wall_core::theme::profiles::ROLE_KEYS
            .iter()
            .map(|key| ((*key).to_string(), json!("#101010")))
            .collect();
    colors.insert("surfaceText".into(), json!("#dbe3e5"));
    std::fs::write(
        cache_root.path().join("colors.json"),
        serde_json::Value::Object(colors).to_string(),
    )
    .unwrap();
    let (desktop, _missing) = desktop(&state);
    assert_eq!(desktop.scene(&[]).palette.text, [219, 227, 229]);
}
