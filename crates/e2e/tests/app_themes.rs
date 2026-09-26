use serde_json::{Value, json};
use skwd_e2e::{Sandbox, Walld, ffmpeg_still, wait_until};
use std::time::Duration;

#[test]
#[ignore = "requires release daemon and fixture renderer"]
fn colour_config_changes_keep_renderer_policy_and_rpc_responsive() {
    let mut sandbox = Sandbox::new("theme-policy");
    sandbox.set_env("SKWD_WALL_PAPER_STILL", &skwd_e2e::stub_renderer!());
    let image = sandbox.library().join("theme.png");
    assert!(ffmpeg_still(&image, "color=c=red:s=320x180"));
    let mut config = json!({
        "paths": {"wallpaper": sandbox.library()}, "restoreOnStartup": false,
        "general": {"randomInterval": 0}, "transition": {"enabled": false},
        "theme": {"policy": "fixed", "mode": "dark", "staticTheme": "nord"}
    });
    sandbox.write_config(&config);
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let applied = client.call("wall.apply", json!({"type": "static", "path": image}), 1).unwrap();
    assert!(applied.get("error").is_none(), "{applied}");
    let path = sandbox.config_path();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for generation in 0..2_000 {
                config["theme"]["staticTheme"] =
                    json!(if generation % 2 == 0 { "nord" } else { "dracula" });
                let staged = path.with_extension("next");
                std::fs::write(&staged, serde_json::to_vec(&config).unwrap()).unwrap();
                std::fs::rename(&staged, &path).unwrap();
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        for request in 2..202 {
            let reply = client.call("wall.retheme", json!({}), request);
            assert!(reply.is_some(), "Config updates blocked the daemon");
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    assert!(walld.responsive());
}

#[test]
#[ignore = "requires release daemon and fixture renderer"]
fn managed_themes_migrate_and_restore_through_rpc() {
    use std::os::unix::fs::PermissionsExt;

    let mut sandbox = Sandbox::new("app-themes");
    let state_home = sandbox.root.join("state");
    sandbox.set_env("XDG_STATE_HOME", state_home.to_str().unwrap());
    let tools = sandbox.root.join("bin");
    std::fs::create_dir_all(&tools).unwrap();
    for name in [
        "fish",
        "kitty",
        "btop",
        "ghostty",
        "niri",
        "rofi",
        "waybar",
        "code",
        "alacritty",
        "yazi",
        "zed",
    ] {
        let path = tools.join(name);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    sandbox.set_env(
        "PATH",
        &format!("{}:{}", tools.display(), std::env::var("PATH").unwrap_or_default()),
    );
    sandbox.set_env("SKWD_WALL_PAPER_STILL", &skwd_e2e::stub_renderer!());
    let image = sandbox.library().join("theme.png");
    assert!(ffmpeg_still(&image, "color=c=red:s=320x180"));
    let config = json!({
        "paths": {"wallpaper": sandbox.library()}, "restoreOnStartup": false,
        "general": {"randomInterval": 0}, "transition": {"enabled": false},
        "theme": {"policy": "fixed", "mode": "dark", "staticTheme": "nord"},
        "integrations": [
            {"name": "kitty", "template": "kitty.conf", "output": "old-kitty.conf"},
            {"name": "kde", "template": "kde-colors.colors", "output": "old-kde.colors"},
            {"name": "vscode", "template": "vscode-theme.json", "output": "old-vscode.json"},
            {"name": "yazi", "template": "yazi-theme.toml", "output": "old-yazi.toml"}
        ]
    });
    sandbox.write_config(&config);
    let data = sandbox.root.join("data");
    sandbox.set_env("XDG_DATA_HOME", data.to_str().unwrap());
    std::fs::create_dir_all(data.join("color-schemes")).unwrap();
    std::fs::write(data.join("color-schemes/BreezeDark.colors"), "[General]\nName=BreezeDark\n")
        .unwrap();
    let kdeglobals = sandbox.root.join("config/kdeglobals");
    std::fs::write(&kdeglobals, "[General]\nColorScheme=BreezeDark\n").unwrap();
    let kde_tool = tools.join("plasma-apply-colorscheme");
    std::fs::write(
        &kde_tool,
        r#"#!/bin/sh
[ ! -f "$XDG_CONFIG_HOME/kde-fail" ] || exit 1
sleep 0.1
printf '[General]\nColorScheme=%s\n' "$1" > "$XDG_CONFIG_HOME/kdeglobals"
"#,
    )
    .unwrap();
    std::fs::set_permissions(&kde_tool, std::fs::Permissions::from_mode(0o700)).unwrap();
    let kitty = sandbox.root.join("config/kitty/kitty.conf");
    std::fs::create_dir_all(kitty.parent().unwrap()).unwrap();
    std::fs::write(&kitty, "font_size 13\n").unwrap();
    let waybar = sandbox.root.join("config/waybar/style.css");
    std::fs::create_dir_all(waybar.parent().unwrap()).unwrap();
    let original_waybar = "window#waybar { color: @primary; }\n";
    std::fs::write(&waybar, format!("{original_waybar}@import 'skwd-colors.css';\n")).unwrap();
    let legacy_output = sandbox.root.join("config/waybar/skwd-colors.css");
    std::fs::write(&legacy_output, "legacy colours").unwrap();
    let legacy_receipt = state_home.join("skwd-wall-v2/app-themes/waybar.json");
    std::fs::create_dir_all(legacy_receipt.parent().unwrap()).unwrap();
    std::fs::write(
        &legacy_receipt,
        serde_json::to_vec(&json!({
            "version": 1, "enabled": true, "pending": false,
            "config": waybar, "output": legacy_output, "original": original_waybar,
            "before": "", "after": "/* Skwd app theme */\n@import \"skwd-colors.css\";\n/* End Skwd app theme */\n",
            "rendered": "legacy colours", "result": "configured"
        })).unwrap(),
    ).unwrap();
    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let invalid = client.call("theme.app.set", json!({"id": "kitty"}), 1).unwrap();
    assert_eq!(invalid["error"]["code"], -32602);
    let listed = client.call("theme.apps", json!({}), 2).unwrap();
    assert!(listed["result"]["apps"].as_array().unwrap().iter().all(|app| app["id"] != "zed"));
    let row = listed["result"]["apps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|app| app["id"] == "kitty")
        .unwrap();
    assert_eq!(row["state"], "conflict");
    assert_eq!(row["can_adopt"], true);
    let applied = client.call("wall.apply", json!({"type": "static", "path": image}), 3).unwrap();
    assert!(applied.get("error").is_none(), "{applied}");
    assert!(wait_until(
        || client
            .call("theme.current", json!({}), 4)
            .is_some_and(|response| response.get("error").is_none()),
        Duration::from_secs(10)
    ));
    let polling = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let readers: Vec<_> = (0..2)
        .map(|_| {
            let polling = polling.clone();
            let mut reader = walld.client();
            std::thread::spawn(move || {
                let mut count = 0;
                while polling.load(std::sync::atomic::Ordering::Acquire) {
                    let reply = reader.call("theme.apps", json!({}), 100);
                    assert!(reply.is_some(), "Theme status blocked during migration");
                    count += 1;
                }
                count
            })
        })
        .collect();
    for id in [
        "fish",
        "kitty",
        "btop",
        "ghostty",
        "niri",
        "rofi",
        "waybar",
        "kde",
        "code",
        "alacritty",
        "yazi",
    ] {
        let response = client
            .call(
                "theme.app.set",
                json!({"id": id, "enabled": true, "adopt": matches!(id, "kitty" | "kde" | "code" | "yazi")}),
                5,
            )
            .unwrap();
        assert!(response.get("error").is_none(), "{id}: {response}");
        let row = response["result"]["apps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == id)
            .unwrap();
        assert_eq!(row["enabled"], true, "{row}");
        let output = row["output_path"].as_str().unwrap();
        assert!(!std::fs::read_to_string(output).unwrap().contains("{{"));
        if id == "kde" {
            let failure = sandbox.root.join("config/kde-fail");
            std::fs::write(&failure, "").unwrap();
            client.call("wall.retheme", json!({}), 7).unwrap();
            let status = |client: &mut skwd_e2e::Client| {
                client.call("theme.apps", json!({}), 8).unwrap()["result"]["apps"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|app| app["id"] == "kde")
                    .unwrap()
                    .clone()
            };
            assert!(wait_until(
                || status(&mut client)["state"] == "interrupted",
                Duration::from_secs(10)
            ));
            std::fs::remove_file(failure).unwrap();
            client.call("wall.retheme", json!({}), 9).unwrap();
            assert!(wait_until(
                || status(&mut client)["state"] == "applied",
                Duration::from_secs(10)
            ));
        }
        if id == "waybar" {
            assert!(output.ends_with("/skwd-theme.css"));
            assert!(!legacy_output.exists());
            let receipt: Value =
                serde_json::from_slice(&std::fs::read(&legacy_receipt).unwrap()).unwrap();
            assert_eq!(receipt["enabled"], false);
            let style = std::fs::read_to_string(&waybar).unwrap();
            assert!(style.contains("skwd-theme.css"));
            assert!(!style.contains("skwd-colors.css"));
        }
        let response =
            client.call("theme.app.set", json!({"id": id, "enabled": false}), 6).unwrap();
        assert!(response.get("error").is_none(), "{id}: {response}");
        assert!(!std::path::Path::new(output).exists());
    }
    polling.store(false, std::sync::atomic::Ordering::Release);
    for reader in readers {
        assert!(reader.join().unwrap() > 0);
    }
    assert_eq!(std::fs::read_to_string(&kitty).unwrap(), "font_size 13\n");
    assert_eq!(std::fs::read_to_string(&waybar).unwrap(), "window#waybar { color: @primary; }\n");
    let saved: Value =
        serde_json::from_str(&std::fs::read_to_string(sandbox.config_path()).unwrap()).unwrap();
    assert_eq!(saved["integrations"][0]["enabled"], false);
    assert_eq!(saved["integrations"][1]["enabled"], false);
    assert_eq!(saved["integrations"][2]["enabled"], false);
    assert_eq!(saved["integrations"][3]["enabled"], false);
    assert!(std::fs::read_to_string(kdeglobals).unwrap().contains("ColorScheme=BreezeDark"));
    assert_eq!(saved["integrations"][0]["template"], "kitty.conf");
    assert!(state_home.join("skwd-wall-v2/app-themes/kitty-migration.json").exists());
    for id in [
        "fish",
        "kitty",
        "btop",
        "ghostty",
        "niri",
        "rofi",
        "waybar",
        "kde",
        "code",
        "alacritty",
        "yazi",
    ] {
        let response = client.call("theme.app.set", json!({"id":id,"enabled":true}), 30).unwrap();
        assert!(response.get("error").is_none(), "{id}: {response}");
        let response = client
            .call("theme.app.customize", json!({"id":id,"action":"create-template"}), 31)
            .unwrap();
        assert!(response.get("error").is_none(), "{id}: {response}");
        let row = response["result"]["apps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|app| app["id"] == id)
            .unwrap();
        let template = row["template_path"].as_str().unwrap();
        assert!(std::path::Path::new(template).exists());
        let output = row["output_path"].as_str().unwrap().to_owned();
        let before = std::fs::read(&output).unwrap();
        let response =
            client.call("theme.app.customize", json!({"id":id,"action":"disconnect"}), 32).unwrap();
        assert!(response.get("error").is_none(), "{id}: {response}");
        let row = response["result"]["apps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|app| app["id"] == id)
            .unwrap();
        assert_eq!(row["state"], "disconnected");
        assert_eq!(row["enabled"], false);
        assert_eq!(std::fs::read(&output).unwrap(), before);
        let response =
            client.call("theme.app.customize", json!({"id":id,"action":"reconnect"}), 33).unwrap();
        assert!(response.get("error").is_none(), "{id}: {response}");
        let row = response["result"]["apps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|app| app["id"] == id)
            .unwrap();
        assert_eq!(row["enabled"], true);
        assert_ne!(row["state"], "disconnected");
        let response = client
            .call("theme.app.customize", json!({"id":id,"action":"reset-template"}), 34)
            .unwrap();
        assert!(response.get("error").is_none(), "{id}: {response}");
        let row = response["result"]["apps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|app| app["id"] == id)
            .unwrap();
        assert_eq!(row["customized"], false);
    }
}
