use super::*;
use serde_json::json;

fn job(id: u64, method: &str) -> (PreviewJob, tokio::sync::mpsc::Receiver<String>) {
    let (sender, receiver) = tokio::sync::mpsc::channel(8);
    (
        PreviewJob {
            request: Request { method: method.into(), params: json!({}), id },
            replies: ResponseSink { sender, close: tokio::sync::watch::channel(false).0 },
            epoch: 0,
            generation: 0,
        },
        receiver,
    )
}

#[test]
fn burst_keeps_latest() {
    let mut state = QueueState::default();
    let (active, _) = job(1, rpc::THEME_PREVIEW);
    assert_eq!(state.push(active).unwrap().request.id, 1);
    let mut superseded = Vec::new();
    for id in 2..=20 {
        let (pending, receiver) = job(id, rpc::THEME_PREVIEW);
        state.push(pending);
        if id < 20 {
            superseded.push(receiver);
        }
    }
    assert_eq!(state.pending.len(), 1);
    assert_eq!(state.pending.pop_front().unwrap().request.id, 20);
    for mut receiver in superseded {
        let response: Response = serde_json::from_str(&receiver.try_recv().unwrap()).unwrap();
        assert_eq!(response.result.unwrap()["superseded"], true);
        assert!(response.error.is_none());
    }
}

#[test]
fn shell_end_keeps_boundary() {
    let mut state = QueueState::default();
    state.push(job(1, rpc::THEME_PREVIEW).0).unwrap();
    let (old, mut old_reply) = job(2, rpc::WALL_SHELL_PREVIEW);
    state.push(old);
    state.push(job(3, rpc::WALL_SHELL_PREVIEW_END).0);
    state.push(job(4, rpc::WALL_SHELL_PREVIEW).0);
    assert_eq!(state.pending.len(), 2);
    assert_eq!(state.pending.pop_front().unwrap().request.id, 3);
    assert_eq!(state.pending.pop_front().unwrap().request.id, 4);
    let response: Response = serde_json::from_str(&old_reply.try_recv().unwrap()).unwrap();
    assert_eq!(response.id, 2);
    assert_eq!(response.result.unwrap()["superseded"], true);
}

#[test]
fn alternating_burst_bounded() {
    let mut state = QueueState::default();
    state.push(job(1, rpc::THEME_PREVIEW).0).unwrap();
    for id in 2..=80 {
        let method = match id % 3 {
            0 => rpc::WALL_SHELL_PREVIEW_END,
            1 => rpc::WALL_SHELL_PREVIEW,
            _ => rpc::THEME_PREVIEW,
        };
        state.push(job(id, method).0);
        assert!(state.pending.len() <= MAX_PENDING_PREVIEWS);
    }
}
