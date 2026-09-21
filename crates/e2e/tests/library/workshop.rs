use super::{
    Client, Duration, Instant, Path, Sandbox, Value, Walld, ffmpeg_still, ffmpeg_video, fs, json,
};

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
