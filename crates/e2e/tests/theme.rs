use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use skwd_e2e::{Checks, Client, Sandbox, Walld, ffmpeg_still, wait_until};
use skwd_wall_core::{material, theme_provider};

const SEED: &str = "#42ff77";
const WAIT: Duration = Duration::from_secs(10);

fn fake(root: &Path, name: &str, body: &str) -> PathBuf {
    let dir = root.join("bin");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn calls(root: &Path) -> String {
    std::fs::read_to_string(root.join("calls")).unwrap_or_default()
}

fn generator_palettes(sandbox: &mut Sandbox, matugen: bool) {
    let root = sandbox.root.to_string_lossy().into_owned();
    sandbox.set_env("SKWD_E2E_PALETTES", &root);
    for (name, seed) in [("a", "#ff0000"), ("b", "#00ff00")] {
        let mut document = material::document_with(seed, true, "tonal-spot").unwrap();
        for mode in ["dark", "light", "default"] {
            document["colors"]["primary"][mode]["color"] = json!(seed);
        }
        let mut tokens = json!({"dark": {}, "light": {}});
        for (role, variants) in document["colors"].as_object().unwrap() {
            for mode in ["dark", "light"] {
                tokens[mode][role] = variants[mode]["color"].clone();
            }
        }
        std::fs::write(
            sandbox.root.join(format!("{name}-native.json")),
            serde_json::to_vec(&json!({"colors": tokens})).unwrap(),
        )
        .unwrap();
        let value = if matugen { document } else { tokens };
        std::fs::write(
            sandbox.root.join(format!("{name}-palette.json")),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_default()
}

fn line_count(path: &Path) -> usize {
    std::fs::read_to_string(path).map_or(0, |text| text.lines().count())
}

fn json_file(path: &Path) -> Value {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null)
}

fn base_config(sandbox: &Sandbox) -> Value {
    json!({
        "paths": {"wallpaper": sandbox.library()},
        "pickOnlyMode": false, "restoreOnStartup": false,
        "general": {"randomInterval": 0}, "transition": {"enabled": false},
        "effects": {"autoRecolor": false, "autoTheme": ""},
    })
}

fn primary_template(sandbox: &Sandbox) -> (PathBuf, PathBuf, PathBuf) {
    let template = sandbox.root.join("primary.tpl");
    let output = sandbox.root.join("primary.out");
    let reloads = sandbox.root.join("reloads");
    std::fs::write(&template, "{{colors.primary.default.hex}}\n").unwrap();
    let reload = sandbox.root.join("reload.sh");
    std::fs::write(&reload, format!("#!/bin/sh\necho reload >> {}\n", reloads.display())).unwrap();
    (template, output, reload)
}

fn wait_event(
    sub: &mut Client,
    timeout: Duration,
    mut accept: impl FnMut(&Value) -> bool,
) -> Option<Value> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let event = sub.recv(remaining.max(Duration::from_millis(50)))?;
        if accept(&event) {
            return Some(event);
        }
    }
    None
}

fn theme_done(event: &Value) -> Option<&Value> {
    (event.get("event") == Some(&json!("skwd.wall.theme_done"))).then(|| &event["data"])?.into()
}

fn fixed_palette() -> (Value, Value) {
    let mut scheme = material::document_with(SEED, true, "tonal-spot").unwrap();
    material::select_mode(&mut scheme, true);
    let mut palette = material::ui_palette(&scheme).unwrap();
    palette["name"] = json!("Edited");
    palette["_scheme"] = scheme.clone();
    palette["_schemeVersion"] = json!(1);
    (scheme, palette)
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn shell_targets_publish_reload_and_import() {
    let stub = skwd_e2e::stub_renderer!();
    let mut sandbox = Sandbox::new("theme-targets");
    sandbox.set_env("SKWD_WALL_PAPER_STILL", &stub);
    sandbox.set_env("SKWD_E2E_CALLS", &sandbox.root.join("calls").to_string_lossy());
    sandbox.set_env("NOCTALIA_CONFIG_HOME", &sandbox.root.join("config").to_string_lossy());
    let image = sandbox.library().join("a.png");
    assert!(ffmpeg_still(&image, "color=c=red:s=320x180"));
    let noctalia = fake(
        &sandbox.root,
        "noctalia",
        r#"if [ "$1" = --version ]; then echo 'noctalia v5.0.0'; exit 0; fi
printf '%s\n' "$*" >> "$SKWD_E2E_CALLS"
if [ "$1" = msg ] && [ "$2" = color-scheme-get ]; then echo 'custom skwd-wall'; fi
exit 0
"#,
    );
    let caelestia = fake(
        &sandbox.root,
        "caelestia",
        r#"if [ "$1" = --version ]; then echo 'caelestia 1.0.0'; exit 0; fi
printf '%s\n' "$*" >> "$SKWD_E2E_CALLS"
exit 0
"#,
    );
    let scheme_json = sandbox.state().join("caelestia/scheme.json");
    std::fs::create_dir_all(scheme_json.parent().unwrap()).unwrap();
    std::fs::write(
        &scheme_json,
        r#"{"name":"native","flavour":"default","mode":"dark","variant":"tonalspot","colours":{"term0":"000000"}}"#,
    )
    .unwrap();
    let (scheme, palette) = fixed_palette();
    let (template, output, reload) = primary_template(&sandbox);
    let reloads = sandbox.root.join("reloads");
    let mut config = base_config(&sandbox);
    config["paths"]["noctaliaBin"] = json!(noctalia);
    config["paths"]["caelestiaBin"] = json!(caelestia);
    config["theme"] = json!({
        "policy": "fixed", "mode": "dark", "scheme": "tonal-spot", "authority": "skwd",
        "staticTheme": "Edited", "savedThemes": [palette],
        "targets": theme_provider::PROVIDERS,
    });
    config["integrations"] =
        json!([{"name": "marker", "template": template, "output": output, "reload": reload}]);
    sandbox.write_config(&config);

    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let mut checks = Checks::default();
    let applied = client.call("wall.apply", json!({"type": "static", "path": image}), 1).unwrap();
    checks.check("static apply accepted", applied.get("error").is_none(), || applied.to_string());

    let provider_files: Vec<(&str, PathBuf)> = [
        ("caelestia", sandbox.state().join("caelestia/scheme.json")),
        ("dms", sandbox.root.join("cache/DankMaterialShell/dms-colors.json")),
        ("noctalia", sandbox.root.join("config/noctalia/palettes/skwd-wall.json")),
        ("end4", sandbox.state().join("quickshell/user/generated/colors.json")),
    ]
    .into_iter()
    .collect();
    for (provider, path) in &provider_files {
        let origin = sandbox.cache().join("theme-origins").join(format!("{provider}.json"));
        checks.check(
            &format!("{provider}: Deck publishes the native palette"),
            wait_until(
                || path.is_file() && !read(path).is_empty() && read(&origin) == read(path),
                WAIT,
            ),
            || format!("{} vs {}\n{}", path.display(), origin.display(), walld.log_contents()),
        );
        let native = json_file(path);
        let canonical = theme_provider::normalize(provider, &native, true);
        checks.check(
            &format!("{provider}: native palette normalizes to the 29-role contract"),
            canonical.as_ref().and_then(Value::as_object).is_some_and(|roles| roles.len() == 29),
            || format!("{native}"),
        );
        checks.check(
            &format!("{provider}: primary round-trips"),
            canonical.as_ref().map(|value| value["primary"].clone())
                == material::role(&scheme, "primary", "dark").map(Value::String),
            || format!("{canonical:?}"),
        );
    }
    checks.check(
        "caelestia keeps its own roles when Deck merges the scheme",
        json_file(&scheme_json)["colours"]["term0"] == json!("000000")
            && json_file(&scheme_json)["name"] == json!("skwd-wall"),
        || json_file(&scheme_json).to_string(),
    );
    checks.check(
        "noctalia target activates the published scheme",
        wait_until(
            || calls(&sandbox.root).contains("msg color-scheme-set custom skwd-wall\n"),
            WAIT,
        ),
        || calls(&sandbox.root),
    );
    let expected_primary = format!("{}\n", material::role(&scheme, "primary", "dark").unwrap());
    checks.check(
        "integration template renders the published primary",
        wait_until(|| read(&output) == expected_primary.as_bytes(), WAIT),
        || String::from_utf8_lossy(&read(&output)).into_owned(),
    );
    checks.check(
        "integration reload command runs after the template is written",
        wait_until(|| line_count(&reloads) >= 1, WAIT),
        || format!("reloads={}", line_count(&reloads)),
    );

    let mut sub = walld.client();
    let subscribed = sub.call("subscribe", json!({}), 40);
    checks.check(
        "event subscription acknowledged",
        subscribed.as_ref().and_then(|value| value.get("result")?.get("subscribed"))
            == Some(&json!(true)),
        || format!("{subscribed:?}"),
    );
    config["theme"]["policy"] = json!("wallpaper");
    config["theme"]["authority"] = json!("caelestia");
    sandbox.write_config(&config);
    std::fs::write(sandbox.root.join("calls"), "").unwrap();
    let retheme = client.call("wall.retheme", json!({}), 2).unwrap();
    checks.check(
        "retheme under caelestia authority accepted",
        retheme.get("error").is_none(),
        || retheme.to_string(),
    );
    let done = wait_event(&mut sub, WAIT, |event| theme_done(event).is_some());
    checks.check(
        "caelestia authority apply reports the caelestia backend",
        done.as_ref().and_then(theme_done).is_some_and(|data| {
            data["backend"] == json!("caelestia")
                && data["ok"] == json!(true)
                && data.get("external").is_none()
        }),
        || format!("{done:?}\n{}", walld.log_contents()),
    );
    let image_str = image.to_string_lossy().into_owned();
    checks.check(
        "caelestia authority drives the shell CLI",
        wait_until(
            || {
                let log = calls(&sandbox.root);
                log.contains(&format!("wallpaper -f {image_str}\n"))
                    && log.contains("scheme set -n dynamic -m dark\n")
            },
            WAIT,
        ),
        || calls(&sandbox.root),
    );

    let mut external = json_file(&scheme_json);
    external["colours"]["primary"] = json!("11aa77");
    std::fs::write(&scheme_json, format!("{}\n", serde_json::to_string_pretty(&external).unwrap()))
        .unwrap();
    let imported = wait_event(&mut sub, WAIT, |event| {
        theme_done(event).is_some_and(|data| data["external"] == json!(true))
    });
    checks.check(
        "external caelestia scheme change is imported through the watcher",
        imported
            .as_ref()
            .and_then(theme_done)
            .is_some_and(|data| data["backend"] == json!("caelestia") && data["ok"] == json!(true)),
        || format!("{imported:?}\n{}", walld.log_contents()),
    );
    let colors = sandbox.cache().join("colors.json");
    checks.check(
        "imported palette reaches the canonical colors.json",
        wait_until(|| json_file(&colors)["primary"] == json!("#11aa77"), WAIT),
        || json_file(&colors).to_string(),
    );
    if checks.failed() {
        sandbox.mark_failed();
    }
    drop(sub);
    drop(client);
    drop(walld);
    checks.finish();
}

fn preview_round_trip(
    checks: &mut Checks,
    walld: &Walld,
    label: &str,
    preview_image: &Path,
    changed: &mut dyn FnMut() -> bool,
    restored: &mut dyn FnMut() -> bool,
) {
    let mut client = walld.client();
    let queued = client.call("wall.shell_preview", json!({"path": preview_image}), 10).unwrap();
    checks.check(
        &format!("{label}: hover preview queued"),
        queued.get("result").and_then(|result| result.get("queued")) == Some(&json!(true)),
        || format!("{queued}"),
    );
    checks.check(
        &format!("{label}: hover preview recolours the shell"),
        wait_until(&mut *changed, WAIT),
        || walld.log_contents(),
    );
    let ended = client.call("wall.shell_preview_end", json!({}), 11).unwrap();
    checks.check(
        &format!("{label}: hover preview end accepted"),
        ended.get("error").is_none(),
        || format!("{ended}"),
    );
    checks.check(
        &format!("{label}: hover preview end restores the shell byte for byte"),
        wait_until(&mut *restored, WAIT),
        || walld.log_contents(),
    );
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn hover_preview_bridge_restores_integrations() {
    let stub = skwd_e2e::stub_renderer!();
    let mut sandbox = Sandbox::new("theme-bridge");
    sandbox.set_env("SKWD_WALL_PAPER_STILL", &stub);
    let red = sandbox.library().join("a.png");
    let green = sandbox.library().join("b.png");
    assert!(ffmpeg_still(&red, "color=c=red:s=320x180"));
    assert!(ffmpeg_still(&green, "color=c=green:s=320x180"));
    let (template, output, reload) = primary_template(&sandbox);
    let reloads = sandbox.root.join("reloads");
    let mut config = base_config(&sandbox);
    config["theme"] =
        json!({"policy": "wallpaper", "authority": "skwd", "engine": "skwd-iris", "mode": "dark"});
    config["integrations"] = json!([{"name": "live", "template": template, "output": output, "reload": reload, "livePreview": true}]);
    sandbox.write_config(&config);

    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let mut checks = Checks::default();
    let applied = client.call("wall.apply", json!({"type": "static", "path": red}), 1).unwrap();
    checks.check("static apply accepted", applied.get("error").is_none(), || applied.to_string());
    let colors = sandbox.cache().join("colors.json");
    checks.check(
        "apply renders the live integration and colors.json",
        wait_until(
            || line_count(&output) == 1 && !read(&colors).is_empty() && line_count(&reloads) >= 1,
            WAIT,
        ),
        || {
            format!(
                "output={:?} reloads={}\n{}",
                read(&output),
                line_count(&reloads),
                walld.log_contents()
            )
        },
    );
    let applied_output = read(&output);
    let applied_colors = read(&colors);
    let applied_reloads = line_count(&reloads);
    preview_round_trip(
        &mut checks,
        &walld,
        "bridge",
        &green,
        &mut || {
            read(&output) != applied_output
                && line_count(&output) == 1
                && read(&colors) != applied_colors
        },
        &mut || read(&output) == applied_output && read(&colors) == applied_colors,
    );
    checks.check(
        "live integration reload runs for the preview and again for the restore",
        wait_until(|| line_count(&reloads) >= applied_reloads + 2, WAIT),
        || format!("reloads={}", line_count(&reloads)),
    );
    if checks.failed() {
        sandbox.mark_failed();
    }
    drop(client);
    drop(walld);
    checks.finish();
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn hover_preview_dms_restores_native_file() {
    let stub = skwd_e2e::stub_renderer!();
    let mut sandbox = Sandbox::new("theme-dms");
    generator_palettes(&mut sandbox, true);
    sandbox.set_env("SKWD_WALL_PAPER_STILL", &stub);
    sandbox.set_env("SKWD_E2E_CALLS", &sandbox.root.join("calls").to_string_lossy());
    let path = std::env::var("PATH").unwrap_or_default();
    sandbox.set_env("PATH", &format!("{}:{path}", sandbox.root.join("bin").display()));
    let templates = sandbox.root.join("config/quickshell/dms/matugen/configs");
    std::fs::create_dir_all(&templates).unwrap();
    std::fs::write(templates.join("base.toml"), "[config]\n").unwrap();
    fake(
        &sandbox.root,
        "dms",
        r#"if [ "$1" = version ]; then echo 'dms v1.0.0'; exit 0; fi
printf '%s\n' "$*" >> "$SKWD_E2E_CALLS"
if [ "$1 $2" = 'matugen check' ]; then echo '[{"id":"gtk"}]'; exit 0; fi
if [ "$1 $2" = 'matugen queue' ]; then
    case "$*" in *b.png*) name=b;; *) name=a;; esac
    mkdir -p "$XDG_CACHE_HOME/DankMaterialShell"
    cp "$SKWD_E2E_PALETTES/$name-native.json" "$XDG_CACHE_HOME/DankMaterialShell/dms-colors.json"
    exit 0
fi
exit 1
"#,
    );
    fake(
        &sandbox.root,
        "matugen",
        r#"if [ "$1" = --version ]; then echo 'matugen 2.4.0'; exit 0; fi
printf '%s\n' "$*" >> "$SKWD_E2E_CALLS"
case "$2" in *b.png) name=b;; *) name=a;; esac
cat "$SKWD_E2E_PALETTES/$name-palette.json"
"#,
    );
    let red = sandbox.library().join("a.png");
    let green = sandbox.library().join("b.png");
    assert!(ffmpeg_still(&red, "color=c=red:s=320x180"));
    assert!(ffmpeg_still(&green, "color=c=green:s=320x180"));
    let mut config = base_config(&sandbox);
    config["theme"] = json!({"policy": "wallpaper", "authority": "dms", "mode": "dark"});
    config["dms"] = json!({"hoverPreview": true});
    sandbox.write_config(&config);

    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let mut checks = Checks::default();
    let applied = client.call("wall.apply", json!({"type": "static", "path": red}), 1).unwrap();
    checks.check("static apply accepted", applied.get("error").is_none(), || applied.to_string());
    let native = sandbox.root.join("cache/DankMaterialShell/dms-colors.json");
    let backup = sandbox.cache().join("dms-preview-orig.json");
    checks.check(
        "dms authority writes dms-colors.json from the generator",
        wait_until(|| json_file(&native)["colors"]["dark"]["primary"] == json!("#ff0000"), WAIT),
        || format!("{}\n{}", json_file(&native), walld.log_contents()),
    );
    let applied_native = read(&native);
    preview_round_trip(
        &mut checks,
        &walld,
        "dms",
        &green,
        &mut || {
            json_file(&native)["colors"]["dark"]["primary"] == json!("#00ff00") && backup.is_file()
        },
        &mut || read(&native) == applied_native && !backup.exists(),
    );
    checks.check(
        "dms preview generates through the configured scheme",
        calls(&sandbox.root).contains("--dry-run -j hex -t scheme-tonal-spot"),
        || calls(&sandbox.root),
    );
    if checks.failed() {
        sandbox.mark_failed();
    }
    drop(client);
    drop(walld);
    checks.finish();
}

#[test]
#[ignore = "e2e: cargo test -p skwd-e2e --release -- --ignored"]
fn hover_preview_noctalia_restores_scheme() {
    let stub = skwd_e2e::stub_renderer!();
    let mut sandbox = Sandbox::new("theme-noctalia");
    generator_palettes(&mut sandbox, false);
    sandbox.set_env("SKWD_WALL_PAPER_STILL", &stub);
    sandbox.set_env("SKWD_E2E_CALLS", &sandbox.root.join("calls").to_string_lossy());
    sandbox.set_env("NOCTALIA_CONFIG_HOME", &sandbox.root.join("config").to_string_lossy());
    let noctalia = fake(
        &sandbox.root,
        "noctalia",
        r#"if [ "$1" = --version ]; then echo 'noctalia v5.0.0'; exit 0; fi
printf '%s\n' "$*" >> "$SKWD_E2E_CALLS"
if [ "$1" = theme ]; then
    case "$2" in *b.png) name=b;; *) name=a;; esac
    cat "$SKWD_E2E_PALETTES/$name-palette.json"
fi
if [ "$1" = msg ] && [ "$2" = color-scheme-get ]; then echo 'wallpaper m3-content'; fi
exit 0
"#,
    );
    let red = sandbox.library().join("a.png");
    let green = sandbox.library().join("b.png");
    assert!(ffmpeg_still(&red, "color=c=red:s=320x180"));
    assert!(ffmpeg_still(&green, "color=c=green:s=320x180"));
    let mut config = base_config(&sandbox);
    config["paths"]["noctaliaBin"] = json!(noctalia);
    config["theme"] = json!({"policy": "wallpaper", "authority": "noctalia", "mode": "dark"});
    config["noctalia"] = json!({"hoverPreview": true, "themeMode": "follow"});
    sandbox.write_config(&config);

    let walld = Walld::start(&sandbox);
    let mut client = walld.client();
    let mut checks = Checks::default();
    let applied = client.call("wall.apply", json!({"type": "static", "path": red}), 1).unwrap();
    checks.check("static apply accepted", applied.get("error").is_none(), || applied.to_string());
    let palettes = sandbox.root.join("config/noctalia/palettes");
    let applied_palette = palettes.join("skwd-wall.json");
    let hover_palette = palettes.join("skwd-hover.json");
    let marker = sandbox.cache().join("noctalia-preview-orig.json");
    checks.check(
        "noctalia authority writes and activates the applied palette",
        wait_until(
            || {
                json_file(&applied_palette)["dark"]["mPrimary"] == json!("#ff0000")
                    && calls(&sandbox.root).contains("msg color-scheme-set custom skwd-wall\n")
            },
            WAIT,
        ),
        || {
            format!(
                "{}\n{}\n{}",
                json_file(&applied_palette),
                calls(&sandbox.root),
                walld.log_contents()
            )
        },
    );
    let applied_bytes = read(&applied_palette);
    preview_round_trip(
        &mut checks,
        &walld,
        "noctalia",
        &green,
        &mut || {
            json_file(&hover_palette)["dark"]["mPrimary"] == json!("#00ff00")
                && marker.is_file()
                && calls(&sandbox.root).contains("msg color-scheme-set custom skwd-hover\n")
        },
        &mut || {
            !hover_palette.exists()
                && !marker.exists()
                && calls(&sandbox.root).contains("msg color-scheme-set wallpaper m3-content\n")
        },
    );
    checks.check(
        "hover preview leaves the applied palette untouched",
        read(&applied_palette) == applied_bytes,
        || String::from_utf8_lossy(&read(&applied_palette)).into_owned(),
    );
    if checks.failed() {
        sandbox.mark_failed();
    }
    drop(client);
    drop(walld);
    checks.finish();
}
