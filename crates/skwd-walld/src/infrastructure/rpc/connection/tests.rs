use super::{MAX_REQUEST_BYTES, read_bounded_line};

#[test]
fn blocked_preview_allows_apply() {
    use std::io::{BufRead, BufReader, Write};
    use std::time::Duration;

    let (_guard, root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({"theme": {"backend": "skwd-iris"}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let image = root.join("walls/blocked-preview.png");
    std::fs::write(&image, b"image fixture").unwrap();
    let serial = state.theme().lock_shell_preview();
    let runtime = crate::testenv::runtime();
    let (client, server) = std::os::unix::net::UnixStream::pair().unwrap();
    client.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let task = runtime.spawn(async move {
        server.set_nonblocking(true).unwrap();
        super::handle_conn(tokio::net::UnixStream::from_std(server).unwrap(), &ctx).await;
    });
    let mut writer = client.try_clone().unwrap();
    let mut reader = BufReader::new(client);
    writeln!(
        writer,
        "{}",
        serde_json::json!({"method": "wall.shell_preview", "id": 8, "params": {"path": image}})
    )
    .unwrap();
    writer.write_all(b"{\"method\":\"wall.apply\",\"id\":9,\"params\":{}}\n").unwrap();
    let mut ids = Vec::new();
    while !ids.contains(&9) {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let response: wall_proto::Response = serde_json::from_str(&line).unwrap();
        ids.push(response.id);
        if response.id == 9 {
            assert_eq!(response.error.unwrap().message, "missing path");
        }
    }
    drop(serial);
    drop(writer);
    drop(reader);
    runtime.block_on(task).unwrap();
}

#[test]
fn slow_palette_allows_apply() {
    use std::io::{BufRead, BufReader, Write};
    use std::time::Duration;

    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let (blocked, release_tx) = block_database(&ctx);
    let runtime = crate::testenv::runtime();
    let (client, server) = std::os::unix::net::UnixStream::pair().unwrap();
    client.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let task = runtime.spawn(async move {
        server.set_nonblocking(true).unwrap();
        super::handle_conn(tokio::net::UnixStream::from_std(server).unwrap(), &ctx).await;
    });
    let mut writer = client.try_clone().unwrap();
    let mut reader = BufReader::new(client);
    writer
        .write_all(
            b"{\"method\":\"theme.preview\",\"id\":8,\"params\":{\"image\":\"/fixture.png\"}}\n",
        )
        .unwrap();
    writer.write_all(b"{\"method\":\"wall.apply\",\"id\":9,\"params\":{}}\n").unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let response: wall_proto::Response = serde_json::from_str(&line).unwrap();
    assert_eq!(response.id, 9);
    assert_eq!(response.error.unwrap().message, "missing path");
    release_tx.send(()).unwrap();
    blocked.join().unwrap();
    drop(writer);
    drop(reader);
    runtime.block_on(task).unwrap();
}

#[test]
fn later_hover_survives_end() {
    use std::io::{BufRead, BufReader, Write};
    use std::time::Duration;

    let (_guard, root) = crate::testenv::lock();
    let _scanner = crate::testenv::scanner(&root);
    crate::testenv::write_config(serde_json::json!({"theme": {"backend": "skwd-iris"}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let colors = palette_path(&state);
    std::fs::write(&colors, b"APPLIED").unwrap();
    let runtime = crate::testenv::runtime();
    let (client, server) = std::os::unix::net::UnixStream::pair().unwrap();
    client.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let task = runtime.spawn(async move {
        server.set_nonblocking(true).unwrap();
        super::handle_conn(tokio::net::UnixStream::from_std(server).unwrap(), &ctx).await;
    });
    let mut writer = client.try_clone().unwrap();
    let mut reader = BufReader::new(client);
    let image = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/outdoors/ambient_day_nps.jpg");
    writeln!(
        writer,
        "{}",
        serde_json::json!({"method": "wall.shell_preview", "id": 1, "params": {"path": image}})
    )
    .unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let first: wall_proto::Response = serde_json::from_str(&line).unwrap();
    assert_eq!(first.id, 1);
    assert_eq!(first.result.unwrap()["queued"], true);
    assert_ne!(std::fs::read(&colors).unwrap(), b"APPLIED");
    writer.write_all(b"{\"method\":\"wall.shell_preview_end\",\"id\":2,\"params\":{}}\n").unwrap();
    writeln!(
        writer,
        "{}",
        serde_json::json!({"method": "wall.shell_preview", "id": 3, "params": {"path": image}})
    )
    .unwrap();
    let mut replies = Vec::new();
    while replies.len() < 2 {
        line.clear();
        reader.read_line(&mut line).unwrap();
        replies.push(serde_json::from_str::<wall_proto::Response>(&line).unwrap());
    }
    assert!(replies.iter().any(|reply| reply.id == 2));
    assert!(replies.iter().any(|reply| reply.id == 3
        && reply.result.as_ref().is_some_and(|result| result["queued"] == true)));
    assert_ne!(std::fs::read(&colors).unwrap(), b"APPLIED");
    writer.write_all(b"{\"method\":\"wall.shell_preview_end\",\"id\":4,\"params\":{}}\n").unwrap();
    line.clear();
    reader.read_line(&mut line).unwrap();
    let ended: wall_proto::Response = serde_json::from_str(&line).unwrap();
    assert_eq!(ended.id, 4);
    assert_eq!(std::fs::read(&colors).unwrap(), b"APPLIED");
    drop(writer);
    drop(reader);
    runtime.block_on(task).unwrap();
}

fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(future)
}

#[test]
fn bounded_reader_normal_line() {
    run(async {
        let mut reader = &b"{\"id\":1}\nnext\n"[..];
        let mut line = Vec::new();
        assert_eq!(read_bounded_line(&mut reader, &mut line).await.unwrap(), 9);
        assert_eq!(line, b"{\"id\":1}\n");
        assert_eq!(read_bounded_line(&mut reader, &mut line).await.unwrap(), 5);
        assert_eq!(line, b"next\n");
    });
}

#[test]
fn bounded_reader_oversized() {
    run(async {
        let input = vec![b'x'; MAX_REQUEST_BYTES + 1];
        let mut reader = &input[..];
        let mut line = Vec::new();
        let error = read_bounded_line(&mut reader, &mut line).await.unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(line.len() <= MAX_REQUEST_BYTES);
    });
}

struct PreviewClient {
    writer: std::os::unix::net::UnixStream,
    reader: std::io::BufReader<std::os::unix::net::UnixStream>,
    task: tokio::task::JoinHandle<()>,
}

impl PreviewClient {
    fn open(runtime: &tokio::runtime::Runtime, ctx: crate::composition::context::Ctx) -> Self {
        let (client, server) = std::os::unix::net::UnixStream::pair().unwrap();
        client.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
        let task = runtime.spawn(async move {
            server.set_nonblocking(true).unwrap();
            super::handle_conn(tokio::net::UnixStream::from_std(server).unwrap(), &ctx).await;
        });
        Self { writer: client.try_clone().unwrap(), reader: std::io::BufReader::new(client), task }
    }

    fn send(&mut self, method: &str, id: u64, params: serde_json::Value) {
        use std::io::Write;
        let request = wall_proto::Request { method: method.to_owned(), id, params };
        writeln!(self.writer, "{}", serde_json::to_string(&request).unwrap()).unwrap();
    }

    fn reply(&mut self) -> wall_proto::Response {
        use std::io::BufRead;
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    fn close(self, runtime: &tokio::runtime::Runtime) {
        drop(self.writer);
        drop(self.reader);
        runtime.block_on(self.task).unwrap();
    }
}

#[test]
fn invalid_hover_preserves_owner() {
    let (_guard, root) = crate::testenv::lock();
    let _scanner = crate::testenv::scanner(&root);
    crate::testenv::write_config(serde_json::json!({"theme": {"backend": "skwd-iris"}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let colors = palette_path(&state);
    std::fs::write(&colors, b"APPLIED").unwrap();
    let runtime = crate::testenv::runtime();
    let mut owner = PreviewClient::open(&runtime, ctx.clone());
    let mut other = PreviewClient::open(&runtime, ctx);
    let image = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/outdoors/ambient_day_nps.jpg");
    owner.send("wall.shell_preview", 1, serde_json::json!({"path": image}));
    assert_eq!(owner.reply().result.unwrap()["queued"], true);
    let hover = std::fs::read(&colors).unwrap();
    assert_ne!(hover, b"APPLIED");
    let generation = state.theme().shell_preview_generation();
    other.send("wall.shell_preview", 2, serde_json::json!({"path": "/missing/preview.png"}));
    assert!(other.reply().error.is_some());
    other.close(&runtime);
    assert_eq!(state.theme().shell_preview_generation(), generation);
    assert_eq!(std::fs::read(&colors).unwrap(), hover);
    owner.send("wall.shell_preview_end", 3, serde_json::json!({}));
    assert!(owner.reply().error.is_none());
    assert_eq!(std::fs::read(&colors).unwrap(), b"APPLIED");
    owner.close(&runtime);
}

#[test]
fn unscannable_hover_fails() {
    let (_guard, root) = crate::testenv::lock();
    let _scanner = crate::testenv::FakeBin::install(
        &root,
        skwd_wall_core::paths::SCAN_BIN,
        "#!/bin/sh\nexit 0\n",
    );
    crate::testenv::write_config(serde_json::json!({"theme": {"backend": "skwd-iris"}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let colors = palette_path(&state);
    std::fs::write(&colors, b"APPLIED").unwrap();
    let image = root.join("walls/unscannable.png");
    std::fs::write(&image, b"not an image").unwrap();
    let runtime = crate::testenv::runtime();
    let mut client = PreviewClient::open(&runtime, ctx);
    client.send("wall.shell_preview", 1, serde_json::json!({"path": image}));
    let reply = client.reply();
    assert_eq!(reply.id, 1);
    assert!(crate::testenv::emsg(&reply).contains("no preview palette"));
    assert_eq!(std::fs::read(&colors).unwrap(), b"APPLIED");
    client.close(&runtime);
    assert!(!state.theme().bridge_preview_armed());
    assert_eq!(std::fs::read(&colors).unwrap(), b"APPLIED");
}

#[test]
fn failed_apply_preserves_end() {
    let (_guard, root) = crate::testenv::lock();
    let _scanner = crate::testenv::scanner(&root);
    crate::testenv::write_config(serde_json::json!({"theme": {"backend": "skwd-iris"}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let colors = palette_path(&state);
    std::fs::write(&colors, b"APPLIED").unwrap();
    let runtime = crate::testenv::runtime();
    let mut client = PreviewClient::open(&runtime, ctx);
    let image = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/outdoors/ambient_day_nps.jpg");
    client.send("wall.shell_preview", 1, serde_json::json!({"path": image}));
    assert_eq!(client.reply().result.unwrap()["queued"], true);
    assert_ne!(std::fs::read(&colors).unwrap(), b"APPLIED");
    let serial = state.theme().lock_shell_preview();
    client.send("wall.shell_preview_end", 2, serde_json::json!({}));
    client.send(
        "wall.apply",
        3,
        serde_json::json!({"path": root.join("missing.png"), "type": "static"}),
    );
    let apply = client.reply();
    assert_eq!(apply.id, 3);
    assert!(apply.error.is_some());
    drop(serial);
    assert_eq!(client.reply().id, 2);
    assert_eq!(std::fs::read(&colors).unwrap(), b"APPLIED");
    client.close(&runtime);
}

#[test]
fn final_theme_rejects_hover() {
    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({"theme": {"backend": "skwd-iris"}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let colors = palette_path(&state);
    let (blocked, release) = block_database(&ctx);
    let runtime = crate::testenv::runtime();
    let mut client = PreviewClient::open(&runtime, ctx);
    let image = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/outdoors/ambient_day_nps.jpg");
    client.send("wall.shell_preview", 1, serde_json::json!({"path": image}));
    client.send("wall.apply", 2, serde_json::json!({}));
    assert_eq!(client.reply().id, 2);
    state.theme().bump_shell_preview();
    std::fs::write(&colors, b"FINAL").unwrap();
    release.send(()).unwrap();
    blocked.join().unwrap();
    assert_eq!(client.reply().result.unwrap()["superseded"], true);
    assert_eq!(std::fs::read(&colors).unwrap(), b"FINAL");
    client.close(&runtime);
    assert_eq!(std::fs::read(&colors).unwrap(), b"FINAL");
}

fn block_database(
    ctx: &crate::composition::context::Ctx,
) -> (std::thread::JoinHandle<()>, std::sync::mpsc::Sender<()>) {
    let database = ctx.database.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let blocked = std::thread::spawn(move || {
        database
            .with_connection(|_| {
                ready_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            })
            .unwrap();
    });
    ready_rx.recv().unwrap();
    (blocked, release_tx)
}

#[test]
fn blocked_end_restores_first() {
    let (_guard, root) = crate::testenv::lock();
    let _scanner = crate::testenv::scanner(&root);
    crate::testenv::write_config(serde_json::json!({"theme": {"backend": "skwd-iris"}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let colors = palette_path(&state);
    std::fs::write(&colors, b"APPLIED").unwrap();
    let runtime = crate::testenv::runtime();
    let mut client = PreviewClient::open(&runtime, ctx.clone());
    let image = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/outdoors/ambient_day_nps.jpg");
    client.send("wall.shell_preview", 1, serde_json::json!({"path": image}));
    assert_eq!(client.reply().result.unwrap()["queued"], true);
    let shell = root.join("cache/DankMaterialShell/dms-colors.json");
    std::fs::create_dir_all(shell.parent().unwrap()).unwrap();
    std::fs::write(&shell, b"DMS APPLIED").unwrap();
    let _matugen = crate::testenv::FakeBin::install(
        &root,
        "matugen",
        "#!/bin/sh\nprintf '%s' '{\"colors\":{\"primary\":{\"dark\":{\"color\":\"#123456\"},\"light\":{\"color\":\"#abcdef\"}}}}'\n",
    );
    crate::testenv::write_config(
        serde_json::json!({"theme": {"backend": "dms"}, "dms": {"hoverPreview": true}}),
    );
    let serial = state.theme().lock_shell_preview();
    let (blocked, release) = block_database(&ctx);
    client.send("wall.shell_preview_end", 2, serde_json::json!({}));
    client.send("wall.shell_preview", 3, serde_json::json!({"path": image}));
    client.send("wall.apply", 4, serde_json::json!({}));
    assert_eq!(client.reply().id, 4);
    drop(serial);
    assert_eq!(client.reply().id, 2);
    assert_eq!(std::fs::read(&colors).unwrap(), b"APPLIED");
    release.send(()).unwrap();
    blocked.join().unwrap();
    assert_eq!(client.reply().result.unwrap()["queued"], true);
    assert_eq!(std::fs::read(&colors).unwrap(), b"APPLIED");
    assert_ne!(std::fs::read(&shell).unwrap(), b"DMS APPLIED");
    client.close(&runtime);
    assert_eq!(std::fs::read(&colors).unwrap(), b"APPLIED");
    assert_eq!(std::fs::read(&shell).unwrap(), b"DMS APPLIED");
}

fn palette_path(state: &skwd_wall_core::state::WallState) -> std::path::PathBuf {
    let cache = std::path::PathBuf::from(state.config().cache_dir());
    std::fs::create_dir_all(&cache).unwrap();
    cache.join("colors.json")
}

#[tokio::test]
async fn shutdown_flushes_reply() {
    use tokio::io::AsyncReadExt;

    let (mut client, server) = tokio::net::UnixStream::pair().unwrap();
    let (_, writer) = server.into_split();
    let (sender, receiver) = tokio::sync::mpsc::channel(2);
    let (drain, draining) = tokio::sync::watch::channel(false);
    let response = wall_proto::Response::err(0, -32600, "oversized request");
    let payload = serde_json::to_string(&response).unwrap();
    sender.try_send(payload.clone()).unwrap();
    drain.send_replace(true);
    let task = tokio::spawn(super::writer_task(writer, receiver, draining));
    let mut received = String::new();
    tokio::time::timeout(super::WRITER_DRAIN_TIMEOUT, client.read_to_string(&mut received))
        .await
        .unwrap()
        .unwrap();
    task.await.unwrap();
    assert_eq!(received, format!("{payload}\n"));
    assert!(sender.is_closed());
}

#[tokio::test]
async fn shutdown_finishes_write() {
    use tokio::io::AsyncReadExt;

    let (mut client, server) = tokio::net::UnixStream::pair().unwrap();
    let (_, writer) = server.into_split();
    let (sender, receiver) = tokio::sync::mpsc::channel(2);
    let (drain, draining) = tokio::sync::watch::channel(false);
    let payload = "x".repeat(MAX_REQUEST_BYTES * 2);
    sender.try_send(payload.clone()).unwrap();
    let task = tokio::spawn(super::writer_task(writer, receiver, draining));
    let mut prefix = [0; 1];
    client.read_exact(&mut prefix).await.unwrap();
    drain.send_replace(true);
    let mut received = String::from_utf8(prefix.to_vec()).unwrap();
    client.read_to_string(&mut received).await.unwrap();
    task.await.unwrap();
    assert_eq!(received, format!("{payload}\n"));
    assert!(sender.is_closed());
}

#[tokio::test]
async fn shutdown_bounds_stall() {
    let (_client, server) = tokio::net::UnixStream::pair().unwrap();
    let (_, writer) = server.into_split();
    let (sender, receiver) = tokio::sync::mpsc::channel(2);
    let (drain, draining) = tokio::sync::watch::channel(false);
    sender.try_send("x".repeat(MAX_REQUEST_BYTES * 2)).unwrap();
    drain.send_replace(true);
    tokio::time::timeout(
        super::WRITER_DRAIN_TIMEOUT * 2,
        super::writer_task(writer, receiver, draining),
    )
    .await
    .unwrap();
    assert!(sender.is_closed());
}

#[test]
fn half_close_flushes_reply() {
    use std::io::BufRead;

    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let runtime = crate::testenv::runtime();
    let mut client = PreviewClient::open(&runtime, ctx);
    client.send("status", 1, serde_json::json!({}));
    client.writer.shutdown(std::net::Shutdown::Write).unwrap();
    assert_eq!(client.reply().id, 1);
    assert_eq!(client.reader.read_line(&mut String::new()).unwrap(), 0);
    client.close(&runtime);
}

#[test]
fn half_close_keeps_preview() {
    use std::io::BufRead;

    let (_guard, _root) = crate::testenv::lock();
    crate::testenv::write_config(serde_json::json!({}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let (blocked, release) = block_database(&ctx);
    let runtime = crate::testenv::runtime();
    let mut client = PreviewClient::open(&runtime, ctx);
    client.send("theme.preview", 1, serde_json::json!({"image": "/fixture.png"}));
    client.send("status", 2, serde_json::json!({}));
    assert_eq!(client.reply().id, 2);
    client.writer.shutdown(std::net::Shutdown::Write).unwrap();
    runtime.block_on(&mut client.task).unwrap();
    release.send(()).unwrap();
    blocked.join().unwrap();
    assert_eq!(client.reply().id, 1);
    assert_eq!(client.reader.read_line(&mut String::new()).unwrap(), 0);
}
