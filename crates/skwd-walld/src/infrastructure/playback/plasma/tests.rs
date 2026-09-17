use super::*;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

async fn connect(path: &Path) -> UnixStream {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(stream) = UnixStream::connect(path).await {
                return stream;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

async fn observe(stream: &mut UnixStream, output: &str, fullscreen: bool, maximized: bool) {
    let value = serde_json::json!({"version":1,"output":output,"supported":true,"fullscreen":fullscreen,"maximized":maximized});
    stream.write_all(format!("{value}\n").as_bytes()).await.unwrap();
}

async fn policy_line(stream: &mut BufReader<UnixStream>) -> (bool, Option<serde_json::Value>) {
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(3), stream.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["version"], 1);
    assert_eq!(value["capabilities"], serde_json::json!(["assignments"]));
    (value["paused"].as_bool().unwrap(), value.get("entry").cloned())
}

async fn until(receiver: &mut watch::Receiver<Snapshot>, predicate: impl Fn(&Snapshot) -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if predicate(&receiver.borrow_and_update()) {
                return;
            }
            receiver.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn native_outputs_update_independently_and_clear_on_disconnect() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("window-state.sock");
    let (sender, mut receiver) = watch::channel(Snapshot::default());
    let task_path = path.clone();
    let task = tokio::spawn(async move { serve(&task_path, sender, policy().subscribe()).await });
    let mut first = connect(&path).await;
    let mut second = connect(&path).await;
    observe(&mut first, "DP-1", true, false).await;
    observe(&mut second, "DP-2", false, true).await;
    until(&mut receiver, |value| {
        value.outputs.contains("DP-1") && value.maximized_outputs.contains("DP-2")
    })
    .await;
    assert_eq!(receiver.borrow().backend, "plasma");
    observe(&mut first, "DP-1", false, false).await;
    until(&mut receiver, |value| {
        value.outputs.is_empty() && value.maximized_outputs.contains("DP-2")
    })
    .await;
    drop(second);
    until(&mut receiver, |value| value.supported && value.maximized_outputs.is_empty()).await;
    drop(first);
    until(&mut receiver, |value| !value.supported).await;
    drop(receiver);
    task.await.unwrap().unwrap();
    assert!(!path.exists());
}

#[tokio::test]
async fn buffered_events_cannot_resurrect_a_closed_connection() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("window-state.sock");
    let (sender, mut receiver) = watch::channel(Snapshot::default());
    let task_path = path.clone();
    let task = tokio::spawn(async move { serve(&task_path, sender, policy().subscribe()).await });
    let mut stream = connect(&path).await;
    observe(&mut stream, "DP-1", true, false).await;
    until(&mut receiver, |value| value.supported).await;
    for _ in 0..100 {
        observe(&mut stream, "DP-1", true, false).await;
    }
    drop(stream);
    until(&mut receiver, |value| !value.supported).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!receiver.borrow().supported);
    drop(receiver);
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn invalid_stream_releases_its_previous_pause() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("window-state.sock");
    let (sender, mut receiver) = watch::channel(Snapshot::default());
    let task_path = path.clone();
    let task = tokio::spawn(async move { serve(&task_path, sender, policy().subscribe()).await });
    let mut stream = connect(&path).await;
    observe(&mut stream, "DP-1", true, false).await;
    until(&mut receiver, |value| value.supported).await;
    stream.write_all(&vec![b'x'; 4097]).await.unwrap();
    until(&mut receiver, |value| !value.supported).await;
    drop(receiver);
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn policy_updates_reach_only_the_assigned_output_and_preserve_partial_observations() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("window-state.sock");
    let (sender, mut receiver) = watch::channel(Snapshot::default());
    let (policies, policy_receiver) =
        watch::channel(HashMap::from([("DP-1".to_owned(), false), ("DP-2".to_owned(), true)]));
    let task_path = path.clone();
    let task = tokio::spawn(async move { serve(&task_path, sender, policy_receiver).await });
    let mut stream = connect(&path).await;
    observe(&mut stream, "DP-1", false, false).await;
    let mut stream = BufReader::new(stream);
    assert_eq!(policy_line(&mut stream).await, (false, None));
    stream.get_mut().write_all(b"{\"version\":1,").await.unwrap();
    policies.send(HashMap::from([("DP-1".to_owned(), true)])).unwrap();
    assert_eq!(policy_line(&mut stream).await, (true, None));
    stream
        .get_mut()
        .write_all(
            b"\"output\":\"DP-1\",\"supported\":true,\"fullscreen\":true,\"maximized\":false}\n",
        )
        .await
        .unwrap();
    until(&mut receiver, |value| value.outputs.contains("DP-1")).await;
    policies.send(HashMap::from([("DP-1".to_owned(), false)])).unwrap();
    assert_eq!(policy_line(&mut stream).await, (false, None));
    drop(stream);
    drop(receiver);
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn binding_preserves_an_existing_non_socket_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("window-state.sock");
    std::fs::write(&path, b"preserve").unwrap();
    assert!(bind(&path).await.is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"preserve");
}

#[test]
fn malformed_or_unversioned_observations_are_rejected() {
    for value in [
        serde_json::json!({"output":"DP-1","supported":true,"fullscreen":true,"maximized":false}),
        serde_json::json!({"version":2,"output":"DP-1","supported":true,"fullscreen":true,"maximized":false}),
        serde_json::json!({"version":1,"output":"","supported":true,"fullscreen":true,"maximized":false}),
        serde_json::json!({"version":1,"output":"DP-1","supported":true,"fullscreen":"yes","maximized":false}),
    ] {
        assert!(Observation::from_value(value).is_err());
    }
}

fn subscribed(output: &str) -> bool {
    skwd_wall_core::plasma::channel::subscribed([output.to_owned()].iter())
}

async fn wait_subscribed(output: &str, expected: bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while subscribed(output) != expected {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn subscribed_output_receives_each_published_assignment_until_it_disconnects() {
    skwd_wall_core::plasma::channel::set_notifier(assignments_published);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("window-state.sock");
    let (sender, receiver) = watch::channel(Snapshot::default());
    let (_policies, policy_receiver) =
        watch::channel(HashMap::from([("SUB-DP-1".to_owned(), false)]));
    let task_path = path.clone();
    let task = tokio::spawn(async move { serve(&task_path, sender, policy_receiver).await });
    let mut stream = connect(&path).await;
    observe(&mut stream, "SUB-DP-1", false, false).await;
    let mut stream = BufReader::new(stream);
    assert_eq!(policy_line(&mut stream).await, (false, None));
    let first = serde_json::json!({"SUB-DP-1": {"assignment": {"source": {"kind": "static", "path": "/a.png"}}}});
    skwd_wall_core::plasma::channel::publish(first.as_object().unwrap());
    stream
        .get_mut()
        .write_all(b"{\"version\":2,\"output\":\"SUB-DP-1\",\"subscribe\":\"assignments\"}\n")
        .await
        .unwrap();
    assert_eq!(policy_line(&mut stream).await, (false, Some(first["SUB-DP-1"].clone())));
    wait_subscribed("SUB-DP-1", true).await;
    skwd_wall_core::plasma::channel::publish(first.as_object().unwrap());
    let second = serde_json::json!({"SUB-DP-1": {"assignment": {"source": {"kind": "static", "path": "/b.png"}}}});
    skwd_wall_core::plasma::channel::publish(second.as_object().unwrap());
    assert_eq!(policy_line(&mut stream).await, (false, Some(second["SUB-DP-1"].clone())));
    drop(stream);
    wait_subscribed("SUB-DP-1", false).await;
    drop(receiver);
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn subscriptions_are_limited_to_the_observed_output() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("window-state.sock");
    let (sender, receiver) = watch::channel(Snapshot::default());
    let (_policies, policy_receiver) =
        watch::channel(HashMap::from([("SUB-DP-2".to_owned(), false)]));
    let task_path = path.clone();
    let task = tokio::spawn(async move { serve(&task_path, sender, policy_receiver).await });
    let mut stream = connect(&path).await;
    observe(&mut stream, "SUB-DP-2", false, false).await;
    let mut stream = BufReader::new(stream);
    assert_eq!(policy_line(&mut stream).await, (false, None));
    stream
        .get_mut()
        .write_all(b"{\"version\":2,\"output\":\"SUB-DP-3\",\"subscribe\":\"assignments\"}\n")
        .await
        .unwrap();
    let mut rest = String::new();
    let closed = tokio::time::timeout(Duration::from_secs(3), stream.read_line(&mut rest))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(closed, 0);
    assert!(!subscribed("SUB-DP-3"));
    drop(receiver);
    task.await.unwrap().unwrap();
}

#[test]
fn subscription_frames_are_strict() {
    let valid = serde_json::json!({"version":2,"output":"DP-1","subscribe":"assignments"});
    assert!(
        matches!(Frame::decode(valid.to_string().as_bytes()), Ok(Frame::Subscribe(output)) if output == "DP-1")
    );
    for value in [
        serde_json::json!({"version":2,"output":"DP-1","subscribe":"everything"}),
        serde_json::json!({"version":2,"output":"","subscribe":"assignments"}),
        serde_json::json!({"version":2,"output":"DP-1","subscribe":"assignments","extra":true}),
        serde_json::json!({"version":3,"output":"DP-1","subscribe":"assignments"}),
    ] {
        assert!(Frame::decode(value.to_string().as_bytes()).is_err(), "{value}");
    }
    let observation = serde_json::json!({"version":1,"output":"DP-1","supported":true,"fullscreen":false,"maximized":false});
    assert!(matches!(Frame::decode(observation.to_string().as_bytes()), Ok(Frame::Observation(_))));
}
