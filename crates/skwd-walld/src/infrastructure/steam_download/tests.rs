#![cfg(test)]

use super::{steam_inflight_begin, steam_inflight_end};
use crate::infrastructure::events::EventHub;
use crate::testenv::{events, subscribe};
use serde_json::json;
use std::path::Path;
use std::sync::Arc;

fn event_hub() -> Arc<EventHub> {
    Arc::new(EventHub::new(Arc::new(crate::infrastructure::stats::Stats::new())))
}

#[test]
fn steamcmd_batch_queues() {
    let subs = event_hub();
    let mut rx = subscribe(&subs);
    let held = super::STEAMCMD_GATE.lock().unwrap();
    let subs2 = Arc::clone(&subs);
    let waiter = std::thread::spawn(move || {
        drop(super::steamcmd_serialize(subs2.as_ref(), &["424242".to_string()]));
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut queued = None;
    while queued.is_none() && std::time::Instant::now() < deadline {
        queued = events(&mut rx).into_iter().find(|ev| ev.data["status"] == json!("queued"));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let queued = queued.expect("queued event");
    assert_eq!(queued.data["id"], json!("424242"));
    drop(held);
    waiter.join().unwrap();
}

#[test]
fn inflight_dedup() {
    assert!(steam_inflight_begin("998877001122"));
    assert!(!steam_inflight_begin("998877001122"));
    steam_inflight_end("998877001122");
    assert!(steam_inflight_begin("998877001122"));
    steam_inflight_end("998877001122");
}

#[test]
fn reconcile_and_finalize() {
    let tmp = tempfile::tempdir().unwrap();
    let we_dir = tmp.path().join("we");
    let actual = tmp.path().join("content/431960/12345");
    std::fs::create_dir_all(&actual).unwrap();
    super::reconcile_we_item(&we_dir, "12345", &actual);
    assert!(we_dir.join("12345").is_dir());
    super::reconcile_we_item(&we_dir, "12345", &actual);
    assert!(we_dir.join("12345").is_dir());
    super::reconcile_we_item(&we_dir, "999", Path::new(""));
    assert!(!we_dir.join("999").exists());

    let subs = event_hub();
    let mut rx = subscribe(&subs);
    let mut folders = std::collections::HashMap::new();
    folders.insert("12345".to_string(), actual.to_string_lossy().into_owned());
    let ids = ["12345".to_string(), "777".to_string()];
    let ok = super::finalize_steam_batch(subs.as_ref(), &we_dir, &ids, "error", "boom", &folders);
    assert!(ok);
    let evs = events(&mut rx);
    let done = evs
        .iter()
        .find(|ev| ev.event == "skwd.wall.download" && ev.data["id"] == json!("12345"))
        .expect("landed item event");
    assert_eq!(done.data["status"], json!("done"));
    let failed = evs
        .iter()
        .find(|ev| ev.event == "skwd.wall.download" && ev.data["id"] == json!("777"))
        .expect("missing item event");
    assert_eq!(failed.data["status"], json!("error"));
    assert_eq!(failed.data["message"], json!("boom"));
}
