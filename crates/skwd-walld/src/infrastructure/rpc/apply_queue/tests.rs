use super::*;
use serde_json::json;

fn job(id: u64, output: &str, kind: &str) -> (QueuedApply, tokio::sync::mpsc::Receiver<String>) {
    let (response, receiver) = tokio::sync::mpsc::channel(8);
    (
        QueuedApply {
            request: Request {
                id,
                method: "wall.apply".into(),
                params: json!({"output": output, "type": kind, "path": format!("/{id}.png"), "we_id": id.to_string()}),
            },
            response: ResponseSink {
                sender: response,
                close: tokio::sync::watch::channel(false).0,
            },
            received: Instant::now(),
        },
        receiver,
    )
}

#[test]
fn active_a_is_preserved_and_pending_b_c_are_replaced_by_d_across_media() {
    for kinds in [
        ["we", "we", "we", "we"],
        ["static", "video", "we", "static"],
        ["video", "static", "we", "video"],
    ] {
        let mut queue = QueueState::default();
        let (a, _a_reply) = job(1, "*", kinds[0]);
        let active = queue.push(a).unwrap();
        let (b, mut b_reply) = job(2, "*", kinds[1]);
        let (c, mut c_reply) = job(3, "*", kinds[2]);
        let (d, _d_reply) = job(4, "*", kinds[3]);
        assert!(queue.push(b).is_none());
        assert!(queue.push(c).is_none());
        assert!(queue.push(d).is_none());
        assert_eq!(active.request.id, 1);
        assert_eq!(queue.next().unwrap().request.id, 4);
        assert!(queue.next().is_none());
        assert!(!queue.running);
        for response in [b_reply.try_recv().unwrap(), c_reply.try_recv().unwrap()] {
            let response: Response = serde_json::from_str(&response).unwrap();
            assert_eq!(response.result.unwrap()["superseded"], true);
            assert!(response.error.is_none());
        }
    }
}

#[test]
fn independent_outputs_and_lock_overrides_are_not_discarded() {
    let mut queue = QueueState::default();
    queue.push(job(1, "*", "we").0).unwrap();
    queue.push(job(2, "DP-1", "we").0);
    queue.push(job(3, "DP-2", "video").0);
    let mut override_job = job(4, "DP-1", "static").0;
    override_job.request.params["override_locks"] = json!(true);
    queue.push(override_job);
    queue.push(job(5, "DP-1", "we").0);
    let mut schedule_job = job(6, "DP-1", "we").0;
    schedule_job.request.params["source"] = json!("schedule");
    queue.push(schedule_job);
    assert_eq!(queue.next().unwrap().request.id, 3);
    assert_eq!(queue.next().unwrap().request.id, 4);
    assert_eq!(queue.next().unwrap().request.id, 5);
    assert_eq!(queue.next().unwrap().request.id, 6);
    assert!(queue.next().is_none());
}

#[test]
fn closing_reply_channels_preserves_latest_pending_work() {
    let mut queue = QueueState::default();
    queue.push(job(1, "*", "we").0).unwrap();
    queue.push(job(2, "*", "we").0);
    queue.push(job(3, "*", "we").0);
    assert_eq!(queue.next().unwrap().request.id, 3);
}

#[test]
fn pending_output_limit_rejects_excess_without_losing_existing_work() {
    let mut queue = QueueState::default();
    queue.push(job(1, "*", "we").0).unwrap();
    for index in 0..MAX_PENDING_APPLIES {
        queue.push(job(index as u64 + 2, &format!("DP-{index}"), "we").0);
    }
    let (excess, mut response) = job(100, "DP-excess", "we");
    queue.push(excess);
    let response: Response = serde_json::from_str(&response.try_recv().unwrap()).unwrap();
    assert!(response.error.is_some());
    assert_eq!(queue.pending.len(), MAX_PENDING_APPLIES);
    queue.push(job(101, "DP-0", "static").0);
    assert_eq!(queue.pending.len(), MAX_PENDING_APPLIES);
    assert_eq!(queue.pending.back().unwrap().request.id, 101);
}

#[test]
fn socket_reader_accepts_latest_selection_while_apply_is_blocked_and_survives_close() {
    use std::io::{BufRead, BufReader, Write};
    use std::time::Duration;

    let (_guard, root) = crate::testenv::lock();
    crate::testenv::write_config(json!({"pickOnlyMode": true, "theme": {"policy": "off"}}));
    let (state, events, stats) = crate::testenv::harness();
    let ctx = crate::testenv::context(&state, &events, &stats);
    let runtime = crate::testenv::runtime();
    let apply_lock = state.apply().lock();
    let mut applied = crate::testenv::subscribe(&events);
    let (mut client, server) = std::os::unix::net::UnixStream::pair().unwrap();
    client.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let task = runtime.spawn(async move {
        server.set_nonblocking(true).unwrap();
        super::super::handle_conn(tokio::net::UnixStream::from_std(server).unwrap(), &ctx).await;
    });
    let mut paths = Vec::new();
    for id in 1..=4 {
        let path = root.join(format!("walls/socket-queue-{id}.png"));
        std::fs::write(&path, b"pick-only fixture").unwrap();
        writeln!(
            client,
            "{}",
            json!({"id": id, "method": "wall.apply", "params": {
                "type": "static", "path": path, "notify": false,
            }})
        )
        .unwrap();
        paths.push(path.to_string_lossy().into_owned());
    }
    writeln!(
        client,
        "{}",
        json!({"id": 5, "method": "wall.apply", "params": {"type": "we", "we_id": ""}})
    )
    .unwrap();
    writeln!(client, "{}", json!({"id": 99, "method": "status"})).unwrap();
    let mut reader = BufReader::new(client);
    loop {
        let mut line = String::new();
        assert_ne!(reader.read_line(&mut line).unwrap(), 0);
        let response: Response = serde_json::from_str(&line).unwrap();
        if response.id == 5 {
            assert_eq!(response.error.unwrap().code, -32602);
            continue;
        }
        if response.id == 99 {
            assert!(response.error.is_none());
            break;
        }
        assert!([2, 3].contains(&response.id));
        assert_eq!(response.result.unwrap()["superseded"], true);
    }
    drop(reader);
    runtime.block_on(task).unwrap();
    drop(apply_lock);
    let observed = runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(3), async {
            let mut observed = Vec::new();
            while let Some(event) = applied.recv().await {
                let event: wall_proto::Event = serde_json::from_str(&event).unwrap();
                if event.event == wall_proto::ev::APPLIED {
                    observed.push(event.data["path"].as_str().unwrap().to_string());
                    if observed.last() == paths.last() {
                        break;
                    }
                }
            }
            observed
        })
        .await
        .unwrap()
    });
    assert_eq!(observed, [paths[0].clone(), paths[3].clone()]);
}
