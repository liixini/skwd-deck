use serde_json::json;
use skwd_e2e::{Checks, Client, Sandbox, Walld, err_code, err_message, wait_until};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn subscriber_count(socket: &std::path::Path) -> i64 {
    let Some(mut client) = Client::connect(socket) else {
        return -1;
    };
    let banner = client
        .call("diag", json!({}), 999)
        .and_then(|resp| resp.get("result")?.get("banner")?.as_str().map(str::to_string))
        .unwrap_or_default();
    banner
        .find("subscribers ")
        .and_then(|at| banner[at + 12..].split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|digits| digits.parse().ok())
        .unwrap_or(-1)
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn rpc_protocol() {
    let mut sandbox = Sandbox::new("rpc");
    let lib = sandbox.library().to_string_lossy().into_owned();
    sandbox.write_config(&json!({
        "paths": { "wallpaper": lib, "videoWallpaper": lib, "steamWorkshop": lib, "steamWeAssets": lib },
        "pickOnlyMode": true,
        "restoreOnStartup": false,
        "general": { "randomInterval": 0 },
        "effects": { "autoRecolor": false, "autoTheme": "" },
    }));
    let walld = Walld::start(&sandbox);
    let socket = walld.socket().to_path_buf();

    let mut checks = Checks::default();
    let mut client = walld.client();

    let status = client.call("status", json!({}), 41);
    checks.check(
        "status advertises Deck service and v1 skwd-wall protocol",
        status.as_ref().is_some_and(|response| {
            let result = &response["result"];
            result["ok"] == json!(true)
                && result["service"]["name"] == json!("skwd-deck")
                && result["service"]["component"] == json!("skwd-walld")
                && result["protocol"]["name"] == json!("skwd-wall")
                && result["protocol"]["version"] == json!(wall_proto::PROTOCOL_VERSION)
                && result["capabilities"] == json!(wall_proto::CAPABILITIES)
        }),
        || format!("{status:?}"),
    );

    let resp = client.call("diag", json!({}), 42);
    checks.check(
        "diag round-trip returns result with echoed id",
        resp.as_ref().is_some_and(|value| {
            value.get("result").is_some() && value.get("id") == Some(&json!(42))
        }),
        || format!("{resp:?}"),
    );

    client.send_raw(b"{this is not json}\n");
    let resp = client.recv(Duration::from_secs(10));
    checks.check("malformed JSON -> error -32700", err_code(resp.as_ref()) == Some(-32700), || {
        format!("{resp:?}")
    });
    checks.check(
        "malformed JSON response carries id 0",
        resp.as_ref().and_then(|value| value.get("id")) == Some(&json!(0)),
        || format!("{resp:?}"),
    );
    let resp = client.call("diag", json!({}), 43);
    checks.check(
        "connection survives a parse error",
        resp.as_ref().is_some_and(|value| {
            value.get("result").is_some() && value.get("id") == Some(&json!(43))
        }),
        || format!("{resp:?}"),
    );

    let resp = client.call("no.such.method", json!({}), 44);
    checks.check("unknown method -> error -32601", err_code(resp.as_ref()) == Some(-32601), || {
        format!("{resp:?}")
    });
    checks.check(
        "unknown method error names the method",
        err_message(resp.as_ref()).contains("no.such.method"),
        || err_message(resp.as_ref()),
    );
    checks.check(
        "unknown method echoes request id",
        resp.as_ref().and_then(|value| value.get("id")) == Some(&json!(44)),
        || format!("{resp:?}"),
    );

    let resp = client.call("wall.apply", json!({ "type": "static", "path": "" }), 45);
    checks.check(
        "apply with empty path -> error -32602 missing path",
        err_code(resp.as_ref()) == Some(-32602) && err_message(resp.as_ref()).contains("path"),
        || format!("{resp:?}"),
    );
    let resp = client.call("wall.apply", json!({ "type": "we" }), 46);
    checks.check(
        "apply type=we without we_id -> error -32602 missing we_id",
        err_code(resp.as_ref()) == Some(-32602) && err_message(resp.as_ref()).contains("we_id"),
        || format!("{resp:?}"),
    );

    client.send_raw(b"\n   \n");
    client.send("diag", json!({}), 47);
    let resp = client.recv(Duration::from_secs(10));
    checks.check(
        "empty/whitespace lines skipped, next request answered",
        resp.as_ref().is_some_and(|value| {
            value.get("id") == Some(&json!(47)) && value.get("result").is_some()
        }),
        || format!("{resp:?}"),
    );
    checks.check(
        "empty lines produce no response",
        client.recv(Duration::from_millis(500)).is_none(),
        String::new,
    );

    let mut oversized = vec![b'x'; 4_000_000];
    oversized.push(b'\n');
    client.send_raw(&oversized);
    let resp = client.recv(Duration::from_secs(10));
    checks.check(
        "oversized line -> bounded request error -32600",
        err_code(resp.as_ref()) == Some(-32600)
            && err_message(resp.as_ref()).contains("1048576 byte limit"),
        || format!("{resp:?}"),
    );
    let resp = client.call("diag", json!({}), 48);
    checks.check("oversized request closes only its connection", resp.is_none(), || {
        format!("{resp:?}")
    });
    drop(client);

    let mut client = walld.client();
    let big =
        json!({ "method": "no.such.method", "params": { "blob": "y".repeat(900_000) }, "id": 49 });
    client.send_raw(big.to_string().as_bytes());
    client.send_raw(b"\n");
    let resp = client.recv(Duration::from_secs(10));
    checks.check(
        "large request below the limit parses normally",
        err_code(resp.as_ref()) == Some(-32601)
            && resp.as_ref().and_then(|value| value.get("id")) == Some(&json!(49)),
        || format!("{resp:?}"),
    );
    let resp = client.call("diag", json!({}), 50);
    checks.check(
        "fresh connection survives a large valid request",
        resp.as_ref().is_some_and(|value| {
            value.get("result").is_some() && value.get("id") == Some(&json!(50))
        }),
        || format!("{resp:?}"),
    );
    drop(client);

    let mut sub = walld.client();
    let resp = sub.call("subscribe", json!({}), 51);
    checks.check(
        "subscribe acks subscribed:true",
        resp.as_ref().and_then(|value| value.get("result")?.get("subscribed"))
            == Some(&json!(true)),
        || format!("{resp:?}"),
    );
    checks.check(
        "subscriber registered (diag counts 1)",
        wait_until(|| subscriber_count(&socket) == 1, Duration::from_secs(4)),
        || format!("count={}", subscriber_count(&socket)),
    );

    let mut other = walld.client();
    let resp = other.call("scan.removed", json!({ "key": "e2e-rpc-marker" }), 52);
    checks.check(
        "event trigger rpc ok",
        resp.as_ref().and_then(|value| value.get("result")?.get("ok")) == Some(&json!(true)),
        || format!("{resp:?}"),
    );
    let ev = sub.recv(Duration::from_secs(5));
    checks.check(
        "subscriber receives skwd.wall.removed event",
        ev.as_ref().is_some_and(|event| {
            event.get("event") == Some(&json!("skwd.wall.removed"))
                && event.get("data").and_then(|data| data.get("key"))
                    == Some(&json!("e2e-rpc-marker"))
        }),
        || format!("{ev:?}"),
    );
    let resp = other.call("scan.item", json!({ "key": "static:e2e-rpc.png" }), 53);
    checks.check(
        "second event trigger rpc ok",
        resp.as_ref().and_then(|value| value.get("result")?.get("ok")) == Some(&json!(true)),
        || format!("{resp:?}"),
    );
    let ev = sub.recv(Duration::from_secs(5));
    checks.check(
        "subscriber receives skwd.wall.cached event",
        ev.as_ref().is_some_and(|event| {
            event.get("event") == Some(&json!("skwd.wall.cached"))
                && event.get("data").and_then(|data| data.get("key"))
                    == Some(&json!("static:e2e-rpc.png"))
        }),
        || format!("{ev:?}"),
    );
    drop(other);
    drop(sub);
    checks.check(
        "subscriber pruned after disconnect (diag counts 0)",
        wait_until(|| subscriber_count(&socket) == 0, Duration::from_secs(6)),
        || format!("count={}", subscriber_count(&socket)),
    );

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let workers: Vec<_> = (0..8)
        .map(|worker| {
            let socket = socket.clone();
            let errors = Arc::clone(&errors);
            std::thread::spawn(move || {
                let base = 1000 * (worker + 1);
                let Some(mut cli) = Client::connect(&socket) else {
                    errors.lock().unwrap().push(format!("base={base}: connect failed"));
                    return;
                };
                for step in 0..10u64 {
                    let id = base + step;
                    let method = if step % 2 == 0 { "wall.outputs" } else { "diag" };
                    let resp = cli.call(method, json!({}), id);
                    let ok = resp.as_ref().is_some_and(|value| {
                        value.get("id") == Some(&json!(id)) && value.get("result").is_some()
                    });
                    if !ok {
                        errors.lock().unwrap().push(format!("id={id} got {resp:?}"));
                        return;
                    }
                }
            })
        })
        .collect();
    for worker in workers {
        let _ = worker.join();
    }
    let concurrent_errors = errors.lock().unwrap();
    checks.check(
        "8 concurrent clients x 10 rpcs: every response matches its request id",
        concurrent_errors.is_empty(),
        || concurrent_errors.iter().take(3).cloned().collect::<Vec<_>>().join("; "),
    );

    checks.check("walld still responsive after protocol abuse", walld.responsive(), String::new);
    checks.check("no panics in walld log", !walld.log_contents().contains("panicked"), String::new);

    if checks.failed() {
        sandbox.mark_failed();
    }
    checks.finish();
}
