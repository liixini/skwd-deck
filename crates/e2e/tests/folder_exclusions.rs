use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use skwd_e2e::{Client, Sandbox, Walld, ffmpeg_still, ffmpeg_video, wait_until};

const WAIT: Duration = Duration::from_secs(30);

fn names(client: &mut Client) -> BTreeSet<String> {
    let reply = client.call(wall_proto::rpc::WALL_LIST, json!({}), 1).unwrap();
    reply["result"]["wallpapers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap().to_string())
        .collect()
}

fn library_event(client: &mut Client) {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        let Some(event) = client.recv(Duration::from_secs(1)) else {
            continue;
        };
        if event["event"] == wall_proto::ev::CONFIG_CHANGED {
            let changed: wall_proto::ev::ConfigChanged =
                serde_json::from_value(event["data"].clone()).unwrap();
            if changed.library_changed {
                return;
            }
        }
    }
    panic!("library event missing");
}

#[test]
#[ignore = "requires release daemon"]
fn exclusions_update_and_restore() {
    let sandbox = Sandbox::new("folder-exclusions");
    let library = sandbox.library();
    let all = ["Sus/a.png", "Sus/deep/b.png", "Sushi/a.png", "Nature/a.png"];
    for name in all {
        let path = library.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        assert!(ffmpeg_still(&path, "color=c=blue:s=64x64"));
    }
    let mut config = json!({
        "paths": {
            "wallpaper": library,
            "videoWallpaper": library,
            "steamWorkshop": sandbox.root.join("empty-workshop"),
        },
        "pickOnlyMode": true,
        "restoreOnStartup": false,
        "general": {"randomInterval": 0},
        "semantic": {"enabled": false},
        "performance": {"autoOptimizeImages": false},
        "effects": {"autoRecolor": false, "autoTheme": ""},
        "library": {"excludedFolders": []},
    });
    sandbox.write_config(&config);
    let daemon = Walld::start(&sandbox);
    let mut client = daemon.client();
    let mut all = all.map(String::from).into_iter().collect::<BTreeSet<_>>();
    assert!(wait_until(|| names(&mut client) == all, WAIT));
    let db = rusqlite::Connection::open(sandbox.sqlite_path()).unwrap();
    db.execute(
        "UPDATE meta SET favourite=1, user_tags='[\"manual\"]', apply_count=7 WHERE name='Sus/a.png'",
        [],
    )
    .unwrap();
    let mut events = daemon.client();
    assert!(events.call("subscribe", json!({}), 2).unwrap()["result"]["subscribed"] == true);
    let visible = ["Sushi/a.png", "Nature/a.png"].map(String::from).into_iter().collect();
    config["library"]["excludedFolders"] = json!(["Sus/"]);
    sandbox.write_config(&config);
    library_event(&mut events);
    assert!(wait_until(|| names(&mut client) == visible, WAIT));
    let retained: (i64, String, i64) = db
        .query_row(
            "SELECT favourite,user_tags,apply_count FROM meta WHERE name='Sus/a.png'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(retained, (1, "[\"manual\"]".into(), 7));
    config["library"]["excludedFolders"] = json!(["Sushi", "Nature"]);
    sandbox.write_config(&config);
    library_event(&mut events);
    let visible = ["Sus/a.png", "Sus/deep/b.png"].map(String::from).into_iter().collect();
    assert!(
        wait_until(|| names(&mut client) == visible, WAIT),
        "expected {visible:?}, got {:?}\n{}",
        names(&mut client),
        daemon.log_contents(),
    );
    drop(events);
    drop(client);
    drop(daemon);
    let daemon = Walld::start(&sandbox);
    let mut client = daemon.client();
    assert_eq!(names(&mut client), visible);
    config["library"]["excludedFolders"] = json!(["Sus"]);
    sandbox.write_config(&config);
    let allowed = ["Sushi/a.png", "Nature/a.png"].map(String::from).into_iter().collect();
    assert!(wait_until(|| names(&mut client) == allowed, WAIT));
    let extra = library.join("Sus/deep/new.png");
    assert!(ffmpeg_still(&extra, "color=c=red:s=64x64"));
    all.insert("Sus/deep/new.png".into());
    config["library"]["excludedFolders"] = Value::Array(Vec::new());
    sandbox.write_config(&config);
    assert!(wait_until(|| names(&mut client) == all, WAIT));
}

#[test]
#[ignore = "requires release daemon"]
fn source_folders_update() {
    let sandbox = Sandbox::new("roots");
    let library = sandbox.library();
    let extra = sandbox.root.join("extra");
    let nested = extra.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    for path in [library.join("same.png"), extra.join("same.png"), nested.join("same.png")] {
        assert!(ffmpeg_still(&path, "color=c=blue:s=64x64"));
    }
    assert!(ffmpeg_video(&extra.join("clip.mp4"), "red", 1.0));
    let mut config = json!({
        "paths":{"wallpaper":library,"videoWallpaper":library,"steamWorkshop":sandbox.root.join("empty-workshop")},
        "library":{"folders":[library, extra, nested]},
        "pickOnlyMode":true,"restoreOnStartup":false,
        "general":{"randomInterval":0},"semantic":{"enabled":false},
        "performance":{"autoOptimizeImages":false},
        "effects":{"autoRecolor":false,"autoTheme":""}
    });
    sandbox.write_config(&config);
    let daemon = Walld::start(&sandbox);
    let mut client = daemon.client();
    let expected: BTreeSet<String> = [
        "same.png".to_string(),
        extra.join("same.png").display().to_string(),
        nested.join("same.png").display().to_string(),
        extra.join("clip.mp4").display().to_string(),
    ]
    .into_iter()
    .collect();
    assert!(wait_until(|| names(&mut client) == expected, WAIT));
    let reply = client.call(wall_proto::rpc::WALL_LIST, json!({}), 2).unwrap();
    let rows = reply["result"]["wallpapers"].as_array().unwrap();
    let thumbs: BTreeSet<&str> = rows.iter().map(|row| row["thumb"].as_str().unwrap()).collect();
    assert_eq!(thumbs.len(), expected.len());
    assert!(rows.iter().any(|row| row["path"] == extra.join("same.png").display().to_string()));
    assert!(
        rows.iter().any(|row| row["video_file"] == extra.join("clip.mp4").display().to_string())
    );
    config["performance"]["autoOptimizeImages"] = json!(true);
    config["library"]["folders"] = json!([extra]);
    sandbox.write_config(&config);
    let mut active = expected.clone();
    active.remove("same.png");
    assert!(wait_until(|| names(&mut client) == active, WAIT));
    let created = extra.join("new.png");
    assert!(ffmpeg_still(&created, "color=c=red:s=64x64"));
    active.insert(created.display().to_string());
    assert!(wait_until(|| names(&mut client) == active, WAIT));
    assert!(created.is_file());
    assert!(extra.join("same.png").is_file());
    assert!(!extra.join(".skwd-wall-v2").exists());
    config["library"]["folders"] = json!([]);
    sandbox.write_config(&config);
    assert!(wait_until(|| names(&mut client).is_empty(), WAIT));
    config["performance"]["autoOptimizeImages"] = json!(false);
    config["library"]["folders"] = json!([nested, extra, library]);
    sandbox.write_config(&config);
    active.insert("same.png".to_string());
    assert!(wait_until(|| names(&mut client) == active, WAIT));
    config["library"]["excludedFolders"] = json!([nested]);
    sandbox.write_config(&config);
    active.remove(&nested.join("same.png").display().to_string());
    assert!(wait_until(|| names(&mut client) == active, WAIT));
}

#[test]
#[ignore = "requires release daemon"]
fn legacy_video_collision() {
    for wallpaper_owner in [false, true] {
        let sandbox = Sandbox::new("owners");
        let library = sandbox.library();
        let videos = sandbox.root.join("videos");
        std::fs::create_dir_all(&videos).unwrap();
        let owner = if wallpaper_owner { &library } else { &videos };
        let other = if wallpaper_owner { &videos } else { &library };
        assert!(ffmpeg_video(&owner.join("same.mp4"), "red", 1.0));
        let mut config = json!({
            "paths":{"wallpaper":library,"videoWallpaper":videos,"steamWorkshop":sandbox.root.join("empty-workshop")},
            "library":{"folders":[owner]},
            "pickOnlyMode":true,"restoreOnStartup":false,
            "general":{"randomInterval":0},"semantic":{"enabled":false},
            "performance":{"autoOptimizeImages":false},
            "effects":{"autoRecolor":false,"autoTheme":""}
        });
        sandbox.write_config(&config);
        let daemon = Walld::start(&sandbox);
        let mut client = daemon.client();
        assert!(wait_until(|| names(&mut client).len() == 1, WAIT));
        let db = rusqlite::Connection::open(sandbox.sqlite_path()).unwrap();
        let old: String = db.query_row("SELECT key FROM meta", [], |row| row.get(0)).unwrap();
        if old != "video:same.mp4" {
            skwd_wall_core::db::rename_wallpaper_key(&db, &old, "video:same.mp4", "same.mp4")
                .unwrap();
        }
        db.execute("UPDATE meta SET favourite=1, apply_count=7", []).unwrap();
        assert!(ffmpeg_video(&other.join("same.mp4"), "blue", 1.0));
        for folders in [json!([videos, library]), json!([]), json!([library, videos])] {
            config["library"]["folders"] = folders.clone();
            sandbox.write_config(&config);
            let count = if folders.as_array().unwrap().is_empty() { 0 } else { 2 };
            assert!(wait_until(|| names(&mut client).len() == count, WAIT));
        }
        let retained: (String, i64, i64) = db
            .query_row(
                "SELECT video_file,favourite,apply_count FROM meta WHERE key='video:same.mp4'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(retained, (owner.join("same.mp4").display().to_string(), 1, 7));
        let other_key: String = db
            .query_row(
                "SELECT key FROM meta WHERE video_file=?1",
                [other.join("same.mp4").display().to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(other_key, format!("video:{}", other.join("same.mp4").display()));
    }
}

#[test]
#[ignore = "requires release daemon"]
fn source_count_transitions() {
    const MANY: usize = 300;
    const TITLE: &str = "Workshop scene";
    let sandbox = Sandbox::new("root-counts");
    let steam = sandbox.root.join("steam");
    let workshop = steam.join("steamapps/workshop/content/431960/710001");
    std::fs::create_dir_all(&workshop).unwrap();
    std::fs::write(workshop.join("scene.json"), "{}").unwrap();
    std::fs::write(
        workshop.join("project.json"),
        json!({"title":TITLE,"type":"scene","file":"scene.json","preview":"preview.png"})
            .to_string(),
    )
    .unwrap();
    assert!(ffmpeg_still(&workshop.join("preview.png"), "color=c=blue:s=64x64"));
    let roots: Vec<_> =
        (0..MANY).map(|index| sandbox.root.join(format!("source-{index}"))).collect();
    for root in &roots {
        std::fs::create_dir_all(root).unwrap();
    }
    let mut config = json!({
        "paths":{"steam":steam,"wallpaper":sandbox.library(),"videoWallpaper":sandbox.library()},
        "library":{"folders":[]},"pickOnlyMode":true,"restoreOnStartup":false,
        "general":{"randomInterval":0},"semantic":{"enabled":false},
        "performance":{"autoOptimizeImages":false},"effects":{"autoRecolor":false,"autoTheme":""}
    });
    sandbox.write_config(&config);
    let daemon = Walld::start(&sandbox);
    let mut client = daemon.client();
    for count in [0, 1, 3, MANY, 0] {
        config["library"]["folders"] = json!(&roots[..count]);
        sandbox.write_config(&config);
        let mut expected = BTreeSet::from([TITLE.to_string()]);
        if count > 0 {
            let file = roots[count - 1].join("last.png");
            assert!(ffmpeg_still(&file, "color=c=red:s=64x64"));
            for root in &roots[..count] {
                let file = root.join("last.png");
                if file.is_file() {
                    expected.insert(file.display().to_string());
                }
            }
        }
        assert!(wait_until(|| names(&mut client) == expected, WAIT), "{}", daemon.log_contents());
    }
}

fn scan_count(events: &mut Client) -> u64 {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if let Some(event) = events.recv(Duration::from_secs(1))
            && event["event"] == wall_proto::ev::SCAN_DONE
        {
            return event["data"]["count"].as_u64().unwrap();
        }
    }
    panic!("scan completion missing");
}

#[test]
#[ignore = "requires release daemon"]
fn source_cache_retained() {
    let sandbox = Sandbox::new("source-cache");
    let library = sandbox.library();
    let extra = sandbox.root.join("posters");
    std::fs::create_dir_all(&extra).unwrap();
    assert!(ffmpeg_still(&library.join("one.png"), "color=c=blue:s=64x64"));
    assert!(ffmpeg_video(&library.join("one.mp4"), "red", 1.0));
    for index in 0..8 {
        let path = extra.join(format!("poster-{index}.jpeg"));
        assert!(ffmpeg_still(&path, "color=c=blue:s=26x38"));
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(100))
            .unwrap();
    }
    let mut config = json!({
        "paths":{"wallpaper":library,"videoWallpaper":library,"steamWorkshop":sandbox.root.join("empty-workshop")},
        "library":{"folders":[library]},
        "pickOnlyMode":true,"restoreOnStartup":false,
        "general":{"randomInterval":0},"semantic":{"enabled":false},
        "performance":{"autoOptimizeImages":false},
        "effects":{"autoRecolor":false,"autoTheme":""}
    });
    sandbox.write_config(&config);
    let daemon = Walld::start(&sandbox);
    let mut client = daemon.client();
    assert!(wait_until(
        || names(&mut client).len() == 2 && daemon.log_contents().contains("scan complete:"),
        WAIT
    ));
    let db = rusqlite::Connection::open(sandbox.sqlite_path()).unwrap();
    db.execute("UPDATE meta SET favourite=1", []).unwrap();
    let reply = client.call(wall_proto::rpc::WALL_LIST, json!({}), 2).unwrap();
    let before = reply["result"]["wallpapers"].as_array().unwrap();
    let artifacts: Vec<_> = before
        .iter()
        .map(|row| {
            let path = std::path::PathBuf::from(row["thumb"].as_str().unwrap());
            let bytes = std::fs::read(&path).unwrap();
            let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
            (path, bytes, modified)
        })
        .collect();
    let mut events = daemon.client();
    assert!(events.call("subscribe", json!({}), 2).unwrap()["result"]["subscribed"] == true);
    for (folders, generated, visible) in [
        (json!([library, extra]), 8, 10),
        (json!([extra, library]), 0, 10),
        (json!([extra]), 0, 8),
        (json!([library, extra]), 0, 10),
    ] {
        config["library"]["folders"] = folders;
        sandbox.write_config(&config);
        assert_eq!(scan_count(&mut events), generated);
        assert_eq!(names(&mut client).len(), visible);
        for (path, bytes, modified) in &artifacts {
            assert_eq!(&std::fs::read(path).unwrap(), bytes);
            assert_eq!(&std::fs::metadata(path).unwrap().modified().unwrap(), modified);
        }
        let favourites: i64 = db
            .query_row("SELECT COUNT(*) FROM meta WHERE favourite=1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(favourites, 2);
    }
    let after = client.call(wall_proto::rpc::WALL_LIST, json!({}), 2).unwrap();
    for row in before {
        let retained = after["result"]["wallpapers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["key"] == row["key"])
            .unwrap();
        for field in ["key", "thumb", "mtime"] {
            assert_eq!(retained[field], row[field]);
        }
    }
}

#[test]
#[ignore = "requires release daemon"]
fn legacy_anchor_replacement() {
    let sandbox = Sandbox::new("anchor-cache");
    let library = sandbox.library();
    let replacement = sandbox.root.join("replacement");
    let extra = sandbox.root.join("extra");
    for directory in [&replacement, &extra] {
        std::fs::create_dir_all(directory).unwrap();
    }
    for (directory, color) in [(&library, "blue"), (&replacement, "red"), (&extra, "green")] {
        let path = directory.join("same.png");
        assert!(ffmpeg_still(&path, &format!("color=c={color}:s=64x64")));
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(100))
            .unwrap();
    }
    let mut config = json!({
        "paths":{"wallpaper":library,"videoWallpaper":library,"steamWorkshop":sandbox.root.join("empty-workshop")},
        "library":{"folders":[library,extra]},
        "pickOnlyMode":true,"restoreOnStartup":false,
        "general":{"randomInterval":0},"semantic":{"enabled":false},
        "performance":{"autoOptimizeImages":false},
        "effects":{"autoRecolor":false,"autoTheme":""}
    });
    sandbox.write_config(&config);
    let daemon = Walld::start(&sandbox);
    let mut client = daemon.client();
    assert!(wait_until(
        || names(&mut client).len() == 2 && daemon.log_contents().contains("scan complete:"),
        WAIT
    ));
    let reply = client.call(wall_proto::rpc::WALL_LIST, json!({}), 2).unwrap();
    let rows = reply["result"]["wallpapers"].as_array().unwrap();
    let legacy = rows.iter().find(|row| row["key"] == "static:same.png").unwrap();
    let external = rows.iter().find(|row| row["key"] != legacy["key"]).unwrap();
    let old = std::fs::read(legacy["thumb"].as_str().unwrap()).unwrap();
    let retained =
        std::fs::metadata(external["thumb"].as_str().unwrap()).unwrap().modified().unwrap();
    let mut events = daemon.client();
    assert!(events.call("subscribe", json!({}), 2).unwrap()["result"]["subscribed"] == true);
    config["paths"]["wallpaper"] = json!(replacement);
    config["library"]["folders"] = json!([replacement, extra]);
    sandbox.write_config(&config);
    assert_eq!(scan_count(&mut events), 1);
    assert_ne!(std::fs::read(legacy["thumb"].as_str().unwrap()).unwrap(), old);
    assert_eq!(
        std::fs::metadata(external["thumb"].as_str().unwrap()).unwrap().modified().unwrap(),
        retained
    );
    let updated = client.call(wall_proto::rpc::WALL_LIST, json!({}), 2).unwrap();
    assert!(
        updated["result"]["wallpapers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["key"] == legacy["key"]
                && row["path"] == replacement.join("same.png").display().to_string())
    );
}

#[test]
#[ignore = "requires release daemon"]
fn same_mtime_exclusions() {
    let sandbox = Sandbox::new("same-mtime");
    let library = sandbox.library();
    for name in ["one", "two"] {
        let directory = library.join(name);
        std::fs::create_dir_all(&directory).unwrap();
        assert!(ffmpeg_still(&directory.join("a.png"), "color=c=blue:s=64x64"));
    }
    let mut config = json!({
        "paths":{"wallpaper":library,"videoWallpaper":library,"steamWorkshop":sandbox.root.join("empty-workshop")},
        "library":{"excludedFolders":["one"]},
        "pickOnlyMode":true,"restoreOnStartup":false,
        "general":{"randomInterval":0},"semantic":{"enabled":false},
        "performance":{"autoOptimizeImages":false},
        "effects":{"autoRecolor":false,"autoTheme":""}
    });
    sandbox.write_config(&config);
    let daemon = Walld::start(&sandbox);
    let mut client = daemon.client();
    let before = ["two/a.png".into()].into_iter().collect();
    assert!(wait_until(
        || names(&mut client) == before && daemon.log_contents().contains("scan complete:"),
        WAIT
    ));
    let metadata = std::fs::metadata(sandbox.config_path()).unwrap();
    let modified = metadata.modified().unwrap();
    config["library"]["excludedFolders"] = json!(["two"]);
    let content = serde_json::to_vec_pretty(&config).unwrap();
    assert!(
        std::process::Command::new("kill")
            .args(["-STOP", &daemon.pid().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let written = std::fs::write(sandbox.config_path(), content).and_then(|()| {
        std::fs::File::options().write(true).open(sandbox.config_path())?.set_modified(modified)
    });
    assert!(
        std::process::Command::new("kill")
            .args(["-CONT", &daemon.pid().to_string()])
            .status()
            .unwrap()
            .success()
    );
    written.unwrap();
    let updated = std::fs::metadata(sandbox.config_path()).unwrap();
    assert_eq!(updated.len(), metadata.len());
    assert_eq!(updated.modified().unwrap(), modified);
    let after = ["one/a.png".into()].into_iter().collect();
    assert!(wait_until(|| names(&mut client) == after, WAIT), "{}", daemon.log_contents());
}
