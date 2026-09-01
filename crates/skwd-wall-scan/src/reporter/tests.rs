#![cfg(test)]

use std::path::PathBuf;

use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use super::Reporter;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("skwd-scan-reporter-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn queued_messages_ordered() {
    let dir = TempDir::new("order");
    let path = dir.0.join("sock");
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    let reporter = Reporter::connect_at(&path).await;
    let (stream, _) = listener.accept().await.unwrap();
    let server = tokio::spawn(async move {
        let mut lines = tokio::io::BufReader::new(stream).lines();
        let mut methods = Vec::new();
        while let Some(line) = lines.next_line().await.unwrap() {
            let value: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(value["id"], 0);
            methods.push(value["method"].as_str().unwrap().to_string());
        }
        lines.get_mut().write_all(b"{\"ok\":true,\"id\":0}\n").await.unwrap();
        methods
    });

    let worker = reporter.clone();
    tokio::task::spawn_blocking(move || {
        worker.send("scan.item", &json!({ "key": "a" }));
        worker.send("scan.item", &json!({ "key": "b" }));
    })
    .await
    .unwrap();
    reporter.send("scan.done", &json!({ "count": 2 }));
    reporter.finish().await;

    let methods = server.await.unwrap();
    assert_eq!(methods, ["scan.item", "scan.item", "scan.done"]);
}

#[tokio::test]
async fn missing_daemon_disables() {
    let dir = TempDir::new("missing");
    let reporter = Reporter::connect_at(&dir.0.join("absent")).await;
    reporter.send("scan.item", &json!({ "key": "a" }));
    reporter.finish().await;
}

#[tokio::test]
async fn daemon_responses_are_drained() {
    let dir = TempDir::new("drain");
    let path = dir.0.join("sock");
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    let reporter = Reporter::connect_at(&path).await;
    let (mut stream, _) = listener.accept().await.unwrap();

    for _ in 0..64 {
        stream.write_all(b"{\"ok\":true,\"id\":0}\n").await.unwrap();
    }
    stream.shutdown().await.unwrap();
    reporter.send("scan.done", &json!({ "count": 0 }));
    reporter.finish().await;

    let mut lines = tokio::io::BufReader::new(stream).lines();
    let line = lines.next_line().await.unwrap().unwrap();
    assert!(line.contains("scan.done"));
}
