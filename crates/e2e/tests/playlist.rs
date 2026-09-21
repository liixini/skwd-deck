use serde_json::json;
use skwd_e2e::{Sandbox, Walld, wait_until};
use std::collections::HashSet;
use std::time::Duration;

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn shuffled_activation_selects_from_all_members() {
    let sandbox = Sandbox::new("playlist-first");
    sandbox.write_config(&json!({
        "paths": { "wallpaper": sandbox.library(), "videoWallpaper": sandbox.library() },
        "pickOnlyMode": true,
        "restoreOnStartup": false,
        "general": { "randomRotate": false },
    }));
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let mut firsts = HashSet::new();
    for trial in 0..64 {
        let response =
            client.call("playlist.create", json!({ "name": "Shuffle start" }), 1).unwrap();
        let id = response["result"]["id"].as_i64().unwrap();
        for member in 0..3 {
            client.call(
                "playlist.add",
                json!({ "id": id, "key": format!("static:{member}.png") }),
                2,
            );
        }
        client.call("playlist.assign", json!({ "id": id, "output": "*" }), 3);
        let response = client.call("wall.playlist.next", json!({ "output": "*" }), 4).unwrap();
        assert_eq!(response["result"]["ok"], true);
        assert!(wait_until(
            || walld.log_lines("playlist: fire").len() > trial,
            Duration::from_secs(5)
        ));
        let lines = walld.log_lines("playlist: fire");
        let selected =
            lines.last().unwrap().split(" -> ").nth(1).unwrap().split_whitespace().next().unwrap();
        firsts.insert(selected.to_string());
    }
    assert_eq!(firsts, (0..3).map(|i| format!("static:{i}.png")).collect());
}
