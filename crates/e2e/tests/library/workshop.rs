use super::{
    Client, Duration, Instant, Path, Sandbox, Value, Walld, ffmpeg_still, ffmpeg_video, fs, json,
};
use std::os::unix::fs::PermissionsExt;

fn wait_rows(walld: &Walld, client: &mut Client, expected: &[(&str, &str)]) -> Vec<Value> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let response = client.call("wall.list", json!({}), 1).expect("library response");
        let rows = response["result"]["wallpapers"].as_array().unwrap();
        if rows.len() == expected.len()
            && expected.iter().all(|(key, title)| {
                rows.iter().any(|row| row["key"] == *key && row["name"] == *title)
            })
        {
            for row in rows {
                for field in ["thumb", "thumb_sm"] {
                    assert!(fs::metadata(row[field].as_str().unwrap()).unwrap().len() > 0);
                }
            }
            return rows.clone();
        }
        assert!(
            Instant::now() < deadline,
            "expected {expected:?}, got {rows:?}\n{}",
            walld.log_contents()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn project(directory: &Path, title: &str, kind: &str, file: &str) {
    fs::create_dir_all(directory).unwrap();
    fs::write(
        directory.join("project.json"),
        serde_json::to_vec(&json!({"title":title,"type":kind,"file":file,"preview":"preview.png"}))
            .unwrap(),
    )
    .unwrap();
}

fn steam_fixture(
    name: &str,
    backend: &str,
) -> (Sandbox, std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let mut sandbox = Sandbox::new(name);
    let steam = sandbox.root.join("steam");
    let workshop = steam.join("steamapps/workshop/content/431960");
    let item = workshop.join("710005");
    project(&item, "Subscribed scene", "scene", "scene.json");
    fs::write(item.join("scene.json"), "{}").unwrap();
    assert!(ffmpeg_still(&item.join("preview.png"), "color=c=blue:s=96x64"));
    let subscribed = sandbox.root.join("subscribed");
    fs::write(&subscribed, "").unwrap();
    let failed = sandbox.root.join("fail");
    let helper = sandbox.root.join("skwd-steam");
    fs::write(
        &helper,
        format!(
            r#"#!/bin/sh
if [ "$1" = inventory ]; then
  if [ -e '{root}/hang-inventory' ]; then sleep 30; fi
  if [ -e '{root}/fail-inventory' ]; then exit 1; fi
  if [ -e '{root}/invalid-inventory' ]; then printf '%s\n' '{{}}'; exit 0; fi
  if [ -e '{root}/empty-inventory' ]; then exit 0; fi
  account=1
  if [ -e '{root}/other-account' ]; then account=2; fi
  if [ -e '{root}/third-account' ]; then account=3; fi
  if [ -e '{root}/child-subscribed' ]; then
    printf '{{"account":"%s","subscribed":["710005","710006"],"installed":{{"710005":"{}","710006":"{}"}}}}\n' "$account"
    exit 0
  fi
  if [ -e '{}' ]; then
    printf '{{"account":"%s","subscribed":["710005"],"installed":{{"710005":"{}"}}}}\n' "$account"
  else
    printf '{{"account":"%s","subscribed":[],"installed":{{"710005":"{}"}}}}\n' "$account"
  fi
  exit 0
fi
if [ "$1" = unsub ]; then
  printf '%s\n' "$2" >> '{root}/unsub-calls'
  if [ -e '{root}/hang-unsub' ]; then sleep 30; fi
  if [ -e '{}' ]; then exit 1; fi
  rm -f '{}'
  exit 0
fi
exit 1
"#,
            item.display(),
            item.parent().unwrap().join("710006").display(),
            subscribed.display(),
            item.display(),
            item.display(),
            failed.display(),
            subscribed.display(),
            root = sandbox.root.display(),
        ),
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
    sandbox.set_env("SKWD_STEAM_BIN", helper.to_str().unwrap());
    sandbox.write_config(&json!({
        "paths":{"steam":steam,"wallpaper":sandbox.library(),"videoWallpaper":sandbox.library()},
        "steam":{"backend":backend},
        "pickOnlyMode":true,"restoreOnStartup":false,"effects":{"autoRecolor":false}
    }));
    (sandbox, item, subscribed, failed)
}

fn secondary_fixture(name: &str) -> (Sandbox, std::path::PathBuf, std::path::PathBuf) {
    let (sandbox, item, subscribed, _) = steam_fixture(name, "steamcmd");
    let secondary = sandbox.root.join("secondary");
    let native = secondary.join("steamapps/workshop/content/431960/710005");
    fs::create_dir_all(native.parent().unwrap()).unwrap();
    fs::rename(&item, &native).unwrap();
    let curated = sandbox.root.join("curated");
    fs::create_dir_all(&curated).unwrap();
    std::os::unix::fs::symlink(&native, curated.join("710005")).unwrap();
    let helper = sandbox.root.join("skwd-steam");
    let script = fs::read_to_string(&helper).unwrap();
    fs::write(
        &helper,
        script.replace(&item.to_string_lossy().to_string(), &native.to_string_lossy()),
    )
    .unwrap();
    fs::write(
        secondary.join("steamapps/workshop/appworkshop_431960.acf"),
        "\"AppWorkshop\"\n{\n\"WorkshopItemsInstalled\"\n{\n\"710005\"\n{\n}\n}\n}",
    )
    .unwrap();
    sandbox.write_config(&json!({
        "paths":{
            "steam":sandbox.root.join("steam"),
            "steamWorkshop":curated,
            "wallpaper":sandbox.library(),
            "videoWallpaper":sandbox.library()
        },
        "steam":{"backend":"steamcmd"},
        "pickOnlyMode":true,"restoreOnStartup":false,"effects":{"autoRecolor":false}
    }));
    (sandbox, native, subscribed)
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn steam_status_connection() {
    let (sandbox, _, _, _) = steam_fixture("s96-steam-status", "steam");
    let failed = sandbox.root.join("fail-inventory");
    fs::write(&failed, "").unwrap();
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let offline = client.call("status", json!({}), 1).unwrap();
    assert_eq!(offline["result"]["steam_helper_available"], true);
    assert_eq!(offline["result"][wall_proto::STEAM_CLIENT_AVAILABLE_FIELD], false);
    fs::remove_file(failed).unwrap();
    let online = client.call("status", json!({}), 2).unwrap();
    assert_eq!(online["result"][wall_proto::STEAM_CLIENT_AVAILABLE_FIELD], true);

    let (sandbox, _, _, _) = steam_fixture("s96-steamcmd-status", "steamcmd");
    let walld = Walld::start(&sandbox);
    let disabled = walld.client().call("status", json!({}), 3).unwrap();
    assert_eq!(disabled["result"]["steam_helper_available"], true);
    assert_eq!(disabled["result"][wall_proto::STEAM_CLIENT_AVAILABLE_FIELD], false);
    let mut config: Value =
        serde_json::from_slice(&fs::read(sandbox.config_path()).unwrap()).unwrap();
    config["steam"]["backend"] = json!("steam");
    sandbox.write_config(&config);
    let enabled = walld.client().call("status", json!({}), 4).unwrap();
    assert_eq!(enabled["result"][wall_proto::STEAM_CLIENT_AVAILABLE_FIELD], true);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn unsubscribe_confirms_before_removal() {
    let (sandbox, item, _subscribed, failed) = steam_fixture("s96-rpc", "steam");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let rows = wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    assert_eq!(rows[0]["steam_subscribed"], true);
    let mut subscriber = walld.client();
    assert_eq!(subscriber.call("subscribe", json!({}), 10).unwrap()["result"]["subscribed"], true);
    fs::write(&failed, "").unwrap();
    let response =
        client.call("wall.remove", json!({"we_id":"710005","unsubscribe":true}), 2).unwrap();
    assert!(response.get("error").is_some());
    let deadline = Instant::now() + Duration::from_secs(5);
    let event = loop {
        let event = subscriber.recv(Duration::from_secs(2)).unwrap();
        if event["event"] == wall_proto::ev::UNSUBSCRIBED {
            break event;
        }
        assert!(Instant::now() < deadline);
    };
    assert_eq!(event["data"]["warn"], false);
    assert_eq!(event["data"]["ok"], false);
    assert!(item.is_dir());
    wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    fs::remove_file(failed).unwrap();
    let response =
        client.call("wall.remove", json!({"we_id":"710005","unsubscribe":true}), 3).unwrap();
    assert_eq!(response["result"]["unsubscribed"], true);
    assert!(item.is_dir());
    wait_rows(&walld, &mut client, &[]);
    drop(walld);
    let restarted = Walld::start(&sandbox);
    wait_rows(&restarted, &mut restarted.client(), &[]);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn dependent_preset_blocks_unsubscribe() {
    let (sandbox, item, subscribed, _) = steam_fixture("s96-dependent-guard", "steam");
    let child = item.parent().unwrap().join("710006");
    fs::create_dir_all(&child).unwrap();
    fs::write(
        child.join("project.json"),
        json!({
            "title":"Dependent preset", "type":"scene", "dependency":"710005",
            "preset":{}, "preview":"preview.png"
        })
        .to_string(),
    )
    .unwrap();
    fs::copy(item.join("preview.png"), child.join("preview.png")).unwrap();
    fs::write(sandbox.root.join("child-subscribed"), "").unwrap();
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let rows = wait_rows(
        &walld,
        &mut client,
        &[("we:710005", "Subscribed scene"), ("we:710006", "Dependent preset")],
    );
    assert!(rows.iter().all(|row| row["steam_subscribed"] == true));
    let response =
        client.call("wall.remove", json!({"we_id":"710005","unsubscribe":true}), 2).unwrap();
    assert!(response["error"]["message"].as_str().unwrap().contains("Dependent preset (710006)"));
    assert!(item.is_dir());
    assert!(child.is_dir());
    assert!(subscribed.is_file());
    assert!(!sandbox.root.join("unsub-calls").exists());
    fs::write(sandbox.root.join("fail-inventory"), "").unwrap();
    let response =
        client.call("wall.remove", json!({"we_id":"710005","unsubscribe":true}), 3).unwrap();
    assert!(response.get("error").is_some());
    assert!(!sandbox.root.join("unsub-calls").exists());
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn manual_numeric_item_survives() {
    let (sandbox, _, _, _) = steam_fixture("s96-manual", "steam");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    let manual = sandbox.root.join("steam/steamapps/workshop/content/431960/710006");
    project(&manual, "Manual scene", "scene", "scene.json");
    fs::write(manual.join("scene.json"), "{}").unwrap();
    assert!(ffmpeg_still(&manual.join("preview.png"), "color=c=blue:s=96x64"));
    let rows = wait_rows(
        &walld,
        &mut client,
        &[("we:710005", "Subscribed scene"), ("we:710006", "Manual scene")],
    );
    assert_eq!(
        rows.iter().find(|row| row["key"] == "we:710005").unwrap()["steam_subscribed"],
        true
    );
    assert!(
        rows.iter()
            .find(|row| row["key"] == "we:710006")
            .unwrap()
            .get("steam_subscribed")
            .is_none()
    );
    assert!(
        rows.iter()
            .find(|row| row["key"] == "we:710006")
            .unwrap()
            .get("steam_confirmation_required")
            .is_none()
    );
    let response =
        client.call("wall.remove", json!({"we_id":"710006","unsubscribe":true}), 2).unwrap();
    assert!(response.get("error").is_some());
    assert!(manual.is_dir());
    wait_rows(
        &walld,
        &mut client,
        &[("we:710005", "Subscribed scene"), ("we:710006", "Manual scene")],
    );
    fs::remove_file(sandbox.root.join("skwd-steam")).unwrap();
    let response =
        client.call("wall.remove", json!({"we_id":"710006","unsubscribe":false}), 3).unwrap();
    assert_eq!(response["result"]["unsubscribed"], false);
    assert!(!manual.exists());
    wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn offline_native_delete_refused() {
    let (sandbox, item, _, _) = steam_fixture("s96-offline-native", "steamcmd");
    let helper = sandbox.root.join("skwd-steam");
    let script = fs::read(&helper).unwrap();
    fs::remove_file(&helper).unwrap();
    let manifest = sandbox.root.join("steam/steamapps/workshop/appworkshop_431960.acf");
    fs::write(
        manifest,
        "\"AppWorkshop\"\n{\n\"WorkshopItemsInstalled\"\n{\n\"710005\"\n{\n}\n}\n}",
    )
    .unwrap();
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let rows = wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    assert!(rows[0].get("steam_subscribed").is_none());
    assert_eq!(rows[0]["steam_confirmation_required"], true);
    let response =
        client.call("wall.remove", json!({"we_id":"710005","unsubscribe":false}), 2).unwrap();
    assert!(response["error"]["message"].as_str().unwrap().contains("status is unknown"));
    assert!(item.is_dir());
    wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    fs::write(&helper, script).unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
    let response =
        client.call("wall.remove", json!({"we_id":"710005","unsubscribe":false}), 3).unwrap();
    assert!(response["error"]["message"].as_str().unwrap().contains("status is unknown"));
    let rows = wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    assert_eq!(rows[0]["steam_subscribed"], true);
    assert_eq!(rows[0]["steam_confirmation_required"], true);
    let response =
        client.call("wall.remove", json!({"we_id":"710005","unsubscribe":true}), 4).unwrap();
    assert_eq!(response["result"]["unsubscribed"], true);
    wait_rows(&walld, &mut client, &[]);
    assert!(item.is_dir());
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn offline_secondary_delete_refused() {
    let (sandbox, native, subscribed) = secondary_fixture("s96-offline-secondary");
    let helper = sandbox.root.join("skwd-steam");
    let script = fs::read(&helper).unwrap();
    fs::remove_file(&helper).unwrap();
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let rows = wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    assert_eq!(rows[0]["steam_confirmation_required"], true);
    let response =
        client.call("wall.remove", json!({"we_id":"710005","unsubscribe":false}), 2).unwrap();
    assert!(response["error"]["message"].as_str().unwrap().contains("status is unknown"));
    assert!(sandbox.root.join("curated/710005").is_symlink());
    assert!(native.is_dir());
    wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    fs::write(&helper, script).unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
    fs::remove_file(subscribed).unwrap();
    let response =
        client.call("wall.remove", json!({"we_id":"710005","unsubscribe":false}), 3).unwrap();
    assert_eq!(response["result"]["unsubscribed"], false);
    assert!(sandbox.root.join("curated/710005").is_symlink());
    assert!(native.is_dir());
    wait_rows(&walld, &mut client, &[]);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn secondary_manifest_unsubscribe() {
    let (sandbox, native, subscribed) = secondary_fixture("s96-secondary-manifest");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let rows = wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    assert_eq!(rows[0]["steam_subscribed"], true);
    fs::remove_file(subscribed).unwrap();
    fs::write(sandbox.root.join("secondary/steamapps/workshop/appworkshop_431960.acf"), "changed")
        .unwrap();
    wait_rows(&walld, &mut client, &[]);
    assert!(native.is_dir());
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn offline_steamcmd_delete() {
    let (sandbox, item, _, _) = steam_fixture("s96-offline-steamcmd", "steamcmd");
    let workshop = sandbox.root.join("steamcmd-items");
    fs::create_dir_all(&workshop).unwrap();
    let local = workshop.join("710005");
    fs::rename(item, &local).unwrap();
    fs::remove_file(sandbox.root.join("skwd-steam")).unwrap();
    fs::write(
        sandbox.root.join("steam/steamapps/workshop/appworkshop_431960.acf"),
        "\"AppWorkshop\"\n{\n\"WorkshopItemsInstalled\"\n{\n\"710005\"\n{\n}\n}\n}",
    )
    .unwrap();
    sandbox.write_config(&json!({
        "paths":{
            "steam":sandbox.root.join("steam"),
            "steamWorkshop":workshop,
            "wallpaper":sandbox.library(),
            "videoWallpaper":sandbox.library()
        },
        "steam":{"backend":"steamcmd"},
        "pickOnlyMode":true,"restoreOnStartup":false,"effects":{"autoRecolor":false}
    }));
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    let response =
        client.call("wall.remove", json!({"we_id":"710005","unsubscribe":false}), 2).unwrap();
    assert_eq!(response["result"]["unsubscribed"], false);
    assert!(!local.exists());
    wait_rows(&walld, &mut client, &[]);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn invalid_inventory_keeps_item() {
    let (sandbox, item, subscribed, _) = steam_fixture("s96-unavailable", "steam");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    fs::write(sandbox.root.join("fail-inventory"), "").unwrap();
    fs::remove_file(subscribed).unwrap();
    let manifest = sandbox.root.join("steam/steamapps/workshop/appworkshop_431960.acf");
    let mut previous = None;
    for mode in ["fail-inventory", "invalid-inventory", "empty-inventory"] {
        let marker = sandbox.root.join(mode);
        fs::write(&marker, "").unwrap();
        if let Some(previous) = previous.replace(marker) {
            fs::remove_file(previous).unwrap();
        }
        fs::write(&manifest, mode).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            assert_eq!(
                client.call("wall.list", json!({}), 2).unwrap()["result"]["wallpapers"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    assert!(item.is_dir());
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn hung_helper_keeps_item() {
    let (sandbox, item, _, _) = steam_fixture("s96-hung", "steam");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    fs::write(sandbox.root.join("hang-unsub"), "").unwrap();
    client.send("wall.remove", json!({"we_id":"710005","unsubscribe":true}), 2);
    let response = client.recv(Duration::from_secs(30)).unwrap();
    assert!(response.get("error").is_some());
    assert!(item.is_dir());
    wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn account_switch_keeps_item() {
    let (sandbox, item, subscribed, _) = steam_fixture("s96-account", "steam");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let rows = wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    assert_eq!(rows[0]["steam_subscribed"], true);
    fs::write(sandbox.root.join("other-account"), "").unwrap();
    fs::remove_file(subscribed).unwrap();
    fs::write(sandbox.root.join("steam/steamapps/workshop/appworkshop_431960.acf"), "changed")
        .unwrap();
    assert!(skwd_e2e::wait_until(
        || {
            let rows = wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
            rows[0].get("steam_subscribed").is_none()
        },
        Duration::from_secs(15)
    ));
    fs::write(sandbox.root.join("third-account"), "").unwrap();
    fs::remove_file(sandbox.root.join("other-account")).unwrap();
    fs::write(sandbox.root.join("steam/steamapps/workshop/appworkshop_431960.acf"), "again")
        .unwrap();
    assert!(skwd_e2e::wait_until(
        || {
            fs::read(sandbox.cache().join("steam-tracking.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .is_some_and(|tracking| tracking["account"] == "3")
        },
        Duration::from_secs(15)
    ));
    wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    fs::remove_file(sandbox.root.join("third-account")).unwrap();
    fs::write(sandbox.root.join("steam/steamapps/workshop/appworkshop_431960.acf"), "back")
        .unwrap();
    wait_rows(&walld, &mut client, &[]);
    assert!(item.is_dir());
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn steamcmd_keeps_native_provenance() {
    let (sandbox, item, _, _) = steam_fixture("s96-steamcmd", "steamcmd");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let rows = wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    assert_eq!(rows[0]["steam_subscribed"], true);
    let response =
        client.call("wall.remove", json!({"we_id":"710005","unsubscribe":true}), 2).unwrap();
    assert_eq!(response["result"]["unsubscribed"], true);
    assert!(item.is_dir());
    wait_rows(&walld, &mut client, &[]);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn external_unsubscribe_survives_restart() {
    let (sandbox, item, subscribed, _) = steam_fixture("s96-external", "steam");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    assert!(walld.wait_log("watching ", Duration::from_secs(10)));
    fs::remove_file(subscribed).unwrap();
    fs::write(sandbox.root.join("steam/steamapps/workshop/appworkshop_431960.acf"), "changed")
        .unwrap();
    wait_rows(&walld, &mut client, &[]);
    assert!(item.is_dir());
    drop(walld);
    let restarted = Walld::start(&sandbox);
    wait_rows(&restarted, &mut restarted.client(), &[]);
}

fn external_unsubscribe_cleans_state(name: &str, partial: bool) {
    let (mut sandbox, item, subscribed, _) = steam_fixture(name, "steam");
    let _plasma = skwd_e2e::FakePlasma::install(
        &mut sandbox,
        env!("CARGO_BIN_EXE_fake_qdbus"),
        env!("CARGO_BIN_EXE_fake_kconfig"),
    );
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let rows = wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    assert_eq!(rows[0]["steam_subscribed"], true);
    let response = client.call("playlist.create", json!({"name":"Saved"}), 2).unwrap();
    let playlist = response["result"]["id"].as_i64().unwrap();
    client.call("playlist.add", json!({"id":playlist,"key":"we:710005"}), 3).unwrap();
    fs::write(
        sandbox.cache().join("last-wallpaper.json"),
        json!({"type":"we","path":item,"we_id":"710005"}).to_string(),
    )
    .unwrap();
    fs::write(
        sandbox.cache().join("last-applied.json"),
        json!({"any":{"type":"we","path":item,"we_id":"710005"}}).to_string(),
    )
    .unwrap();
    fs::write(
        sandbox.cache().join("outputs.json"),
        json!({"DP-1":{"type":"we","path":item,"we_id":"710005"}}).to_string(),
    )
    .unwrap();
    let assignments = sandbox.root.join("plasma/assignments.json");
    fs::write(
        &assignments,
        json!({"activity-a":{"DP-1":{"wallpaper":{"path":item.join("scene.json"),"we_id":""}}}})
            .to_string(),
    )
    .unwrap();
    assert!(walld.wait_log("watching ", Duration::from_secs(10)));
    fs::remove_file(subscribed).unwrap();
    if partial {
        fs::remove_file(item.join("project.json")).unwrap();
        assert!(item.join("scene.json").exists());
    } else {
        fs::remove_dir_all(&item).unwrap();
    }
    wait_rows(&walld, &mut client, &[]);
    assert!(
        skwd_e2e::wait_until(
            || {
                let memberships =
                    client.call("playlist.memberships", json!({"key":"we:710005"}), 4).unwrap();
                memberships["result"]["ids"].as_array().unwrap().is_empty()
                    && !sandbox.cache().join("last-wallpaper.json").exists()
                    && fs::read(sandbox.cache().join("last-applied.json"))
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                        .is_some_and(|value| value["any"].is_null())
                    && sandbox.outputs_json().as_object().is_some_and(serde_json::Map::is_empty)
                    && fs::read(&assignments)
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                        .is_some_and(|value| value["activity-a"].get("DP-1").is_none())
            },
            Duration::from_secs(15)
        ),
        "{}",
        walld.log_contents()
    );
    drop(walld);
    let restarted = Walld::start(&sandbox);
    let mut client = restarted.client();
    wait_rows(&restarted, &mut client, &[]);
    let memberships = client.call("playlist.memberships", json!({"key":"we:710005"}), 5).unwrap();
    assert!(memberships["result"]["ids"].as_array().unwrap().is_empty());
    assert_eq!(item.exists(), partial);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn external_unsubscribe_removes_folder() {
    external_unsubscribe_cleans_state("s96-external-gone", false);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn external_unsubscribe_removes_project() {
    external_unsubscribe_cleans_state("s96-external-partial", true);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn external_missing_guards() {
    let (sandbox, item, subscribed, _) = steam_fixture("s96-external-guard", "steam");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    let response = client.call("playlist.create", json!({"name":"Saved"}), 2).unwrap();
    let playlist = response["result"]["id"].as_i64().unwrap();
    client.call("playlist.add", json!({"id":playlist,"key":"we:710005"}), 3).unwrap();
    fs::write(
        sandbox.cache().join("last-wallpaper.json"),
        json!({"type":"we","path":item,"we_id":"710005"}).to_string(),
    )
    .unwrap();
    fs::write(sandbox.root.join("fail-inventory"), "").unwrap();
    fs::remove_file(subscribed).unwrap();
    fs::remove_dir_all(&item).unwrap();
    wait_rows(&walld, &mut client, &[]);
    let memberships = client.call("playlist.memberships", json!({"key":"we:710005"}), 4).unwrap();
    assert_eq!(memberships["result"]["ids"], json!([playlist]));
    assert!(sandbox.cache().join("last-wallpaper.json").is_file());
    fs::remove_file(sandbox.root.join("fail-inventory")).unwrap();
    fs::write(sandbox.root.join("other-account"), "").unwrap();
    fs::write(sandbox.root.join("steam/steamapps/workshop/appworkshop_431960.acf"), "changed")
        .unwrap();
    assert!(skwd_e2e::wait_until(
        || fs::read(sandbox.cache().join("steam-tracking.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|tracking| tracking["account"] == "2"),
        Duration::from_secs(15)
    ));
    let memberships = client.call("playlist.memberships", json!({"key":"we:710005"}), 5).unwrap();
    assert_eq!(memberships["result"]["ids"], json!([playlist]));
    assert!(sandbox.cache().join("last-wallpaper.json").is_file());
    fs::remove_file(sandbox.root.join("other-account")).unwrap();
    fs::write(sandbox.root.join("steam/steamapps/workshop/appworkshop_431960.acf"), "returned")
        .unwrap();
    assert!(skwd_e2e::wait_until(
        || {
            let memberships =
                client.call("playlist.memberships", json!({"key":"we:710005"}), 6).unwrap();
            memberships["result"]["ids"].as_array().unwrap().is_empty()
                && !sandbox.cache().join("last-wallpaper.json").exists()
        },
        Duration::from_secs(15)
    ));
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn hidden_account_return() {
    let (sandbox, item, subscribed, _) = steam_fixture("s96-hidden-account", "steam");
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    let response = client.call("playlist.create", json!({"name":"Saved"}), 2).unwrap();
    let playlist = response["result"]["id"].as_i64().unwrap();
    client.call("playlist.add", json!({"id":playlist,"key":"we:710005"}), 3).unwrap();
    drop(walld);

    fs::remove_file(subscribed).unwrap();
    fs::remove_dir_all(&item).unwrap();
    fs::write(
        sandbox.cache().join("steam-tracking.json"),
        json!({
            "account":"1", "source":item.parent().unwrap(), "seen":{},
            "hidden":{"710005":item}, "owned":{"1":{"710005":item}},
            "active":{"1":{}}
        })
        .to_string(),
    )
    .unwrap();
    fs::write(
        sandbox.cache().join("last-wallpaper.json"),
        json!({"type":"we","path":item,"we_id":"710005"}).to_string(),
    )
    .unwrap();
    fs::write(sandbox.root.join("other-account"), "").unwrap();
    let restarted = Walld::start(&sandbox);
    let mut client = restarted.client();
    wait_rows(&restarted, &mut client, &[]);
    let memberships = client.call("playlist.memberships", json!({"key":"we:710005"}), 4).unwrap();
    assert_eq!(memberships["result"]["ids"], json!([playlist]));
    assert!(sandbox.cache().join("last-wallpaper.json").is_file());
    fs::remove_file(sandbox.root.join("other-account")).unwrap();
    fs::write(sandbox.root.join("steam/steamapps/workshop/appworkshop_431960.acf"), "returned")
        .unwrap();
    assert!(skwd_e2e::wait_until(
        || {
            let memberships =
                client.call("playlist.memberships", json!({"key":"we:710005"}), 5).unwrap();
            memberships["result"]["ids"].as_array().unwrap().is_empty()
                && !sandbox.cache().join("last-wallpaper.json").exists()
        },
        Duration::from_secs(15)
    ));
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn hidden_cleanup_retries() {
    let (mut sandbox, item, subscribed, _) = steam_fixture("s96-hidden-retry", "steamcmd");
    let _plasma = skwd_e2e::FakePlasma::install(
        &mut sandbox,
        env!("CARGO_BIN_EXE_fake_qdbus"),
        env!("CARGO_BIN_EXE_fake_kconfig"),
    );
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    wait_rows(&walld, &mut client, &[("we:710005", "Subscribed scene")]);
    let response = client.call("playlist.create", json!({"name":"Saved"}), 2).unwrap();
    let playlist = response["result"]["id"].as_i64().unwrap();
    client.call("playlist.add", json!({"id":playlist,"key":"we:710005"}), 3).unwrap();
    drop(walld);

    fs::remove_file(subscribed).unwrap();
    fs::remove_file(sandbox.root.join("skwd-steam")).unwrap();
    let source = item.parent().unwrap();
    fs::write(
        sandbox.cache().join("steam-tracking.json"),
        json!({
            "account":"1","source":source,"seen":{},
            "hidden":{"710005":item},
            "owned":{"1":{"710005":item}},"active":{"1":{}}
        })
        .to_string(),
    )
    .unwrap();
    fs::write(
        sandbox.cache().join("last-wallpaper.json"),
        json!({"type":"we","path":item,"we_id":"710005"}).to_string(),
    )
    .unwrap();
    fs::write(
        sandbox.cache().join("last-applied.json"),
        json!({"any":{"type":"we","path":item,"we_id":"710005"}}).to_string(),
    )
    .unwrap();
    let assignments = sandbox.root.join("plasma/assignments.json");
    fs::write(
        &assignments,
        json!({"activity-a":{"DP-1":{"wallpaper":{"path":item.join("scene.json"),"we_id":""}}}})
            .to_string(),
    )
    .unwrap();
    fs::remove_dir_all(&item).unwrap();

    let restarted = Walld::start(&sandbox);
    let mut client = restarted.client();
    wait_rows(&restarted, &mut client, &[]);
    let memberships = client.call("playlist.memberships", json!({"key":"we:710005"}), 4).unwrap();
    assert!(memberships["result"]["ids"].as_array().unwrap().is_empty());
    assert!(!sandbox.cache().join("last-wallpaper.json").exists());
    let last: Value =
        serde_json::from_slice(&fs::read(sandbox.cache().join("last-applied.json")).unwrap())
            .unwrap();
    assert!(last["any"].is_null());
    let after: Value = serde_json::from_slice(&fs::read(assignments).unwrap()).unwrap();
    assert!(after["activity-a"].get("DP-1").is_none());
    assert!(!item.exists());
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn steam_workshop_downloads_import_without_refresh() {
    let mut sandbox = Sandbox::new("workshop");
    let workshop = sandbox.root.join("steam/steamapps/workshop/content/431960");
    fs::create_dir_all(&workshop).unwrap();
    sandbox.set_env("HOME", sandbox.root.join("home").to_str().unwrap());
    let preview = sandbox.root.join("preview.png");
    assert!(ffmpeg_still(&preview, "color=c=blue:s=96x64"));
    let video = sandbox.root.join("video.mp4");
    assert!(ffmpeg_video(&video, "green", 0.5));
    let mut config = json!({
        "paths":{"steam":sandbox.root.join("steam"),"wallpaper":sandbox.library(),"videoWallpaper":sandbox.library()},
        "pickOnlyMode":true,"restoreOnStartup":false,"effects":{"autoRecolor":false}
    });
    sandbox.write_config(&config);
    let walld = Walld::start(&sandbox);
    assert!(walld.wait_log("scan complete:", Duration::from_secs(10)));
    let mut client = walld.client();
    wait_rows(&walld, &mut client, &[]);

    let staged = sandbox.root.join("downloads/710001");
    project(&staged, "Downloaded scene", "scene", "scene.json");
    fs::write(staged.join("scene.json"), "{}").unwrap();
    fs::copy(&preview, staged.join("preview.png")).unwrap();
    fs::rename(&staged, workshop.join("710001")).unwrap();
    wait_rows(&walld, &mut client, &[("we:710001", "Downloaded scene")]);

    let arriving = workshop.join("710002");
    project(&arriving, "Delayed preview", "scene", "scene.json");
    let rows = wait_rows(
        &walld,
        &mut client,
        &[("we:710001", "Downloaded scene"), ("we:710002", "Delayed preview")],
    );
    assert_eq!(rows.iter().find(|row| row["key"] == "we:710002").unwrap()["width"], 0);
    fs::write(arriving.join("scene.json"), "{}").unwrap();
    fs::copy(&preview, arriving.join("preview.png")).unwrap();
    assert!(
        skwd_e2e::wait_until(
            || {
                let rows = wait_rows(
                    &walld,
                    &mut client,
                    &[("we:710001", "Downloaded scene"), ("we:710002", "Delayed preview")],
                );
                rows.iter().any(|row| row["key"] == "we:710002" && row["width"] == 96)
            },
            Duration::from_secs(20),
        ),
        "late preview did not replace the placeholder\n{}",
        walld.log_contents()
    );

    let movie = workshop.join("710003");
    project(&movie, "Downloaded video", "video", "movie.mp4");
    fs::copy(&video, movie.join("movie.mp4")).unwrap();
    let rows = wait_rows(
        &walld,
        &mut client,
        &[
            ("we:710001", "Downloaded scene"),
            ("we:710002", "Delayed preview"),
            ("we:710003", "Downloaded video"),
        ],
    );
    assert_eq!(rows.iter().find(|row| row["key"] == "we:710003").unwrap()["type"], "video");

    project(&workshop.join("710001"), "Updated scene", "scene", "scene.json");
    wait_rows(
        &walld,
        &mut client,
        &[
            ("we:710001", "Updated scene"),
            ("we:710002", "Delayed preview"),
            ("we:710003", "Downloaded video"),
        ],
    );
    fs::remove_dir_all(&arriving).unwrap();
    wait_rows(
        &walld,
        &mut client,
        &[("we:710001", "Updated scene"), ("we:710003", "Downloaded video")],
    );

    let custom = sandbox.root.join("Custom Workshop 桜");
    fs::create_dir_all(&custom).unwrap();
    config["paths"]["steamWorkshop"] = json!(custom);
    sandbox.write_config(&config);
    wait_rows(&walld, &mut client, &[]);
    fs::rename(&movie, custom.join("710003")).unwrap();
    wait_rows(&walld, &mut client, &[("we:710003", "Downloaded video")]);
    fs::remove_file(custom.join("710003/project.json")).unwrap();
    wait_rows(&walld, &mut client, &[]);
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn symlink_unsubscribe_prunes() {
    let sandbox = Sandbox::new("s96");
    let workshop = sandbox.root.join("workshop");
    let installed = sandbox.root.join("steam-library/710004");
    fs::create_dir_all(&workshop).unwrap();
    sandbox.write_config(&json!({
        "paths":{"steamWorkshop":workshop,"wallpaper":sandbox.library(),"videoWallpaper":sandbox.library()},
        "pickOnlyMode":true,"restoreOnStartup":false,"effects":{"autoRecolor":false}
    }));
    let walld = Walld::start(&sandbox);
    assert!(walld.wait_log("scan complete:", Duration::from_secs(10)));
    let mut client = walld.client();
    wait_rows(&walld, &mut client, &[]);

    project(&installed, "Steam scene", "scene", "scene.json");
    fs::write(installed.join("scene.json"), "{}").unwrap();
    assert!(ffmpeg_still(&installed.join("preview.png"), "color=c=blue:s=96x64"));
    std::os::unix::fs::symlink(&installed, workshop.join("710004")).unwrap();
    wait_rows(&walld, &mut client, &[("we:710004", "Steam scene")]);

    fs::remove_dir_all(&installed).unwrap();
    assert!(
        skwd_e2e::wait_until(
            || {
                let response = client.call("wall.list", json!({}), 1).unwrap();
                response["result"]["wallpapers"].as_array().unwrap().is_empty()
            },
            Duration::from_secs(10),
        ),
        "stale Workshop symlink row\n{}",
        walld.log_contents()
    );
}
