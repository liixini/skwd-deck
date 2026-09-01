#![cfg(test)]

use super::*;
use serde_json::json;

#[test]
fn static_custom_forms() {
    let defaults = Config::from_root(json!({}));
    assert_eq!(defaults.theme().static_theme(), "nord");
    assert!(defaults.theme().static_custom().is_empty());

    let arr = Config::from_root(json!({"theme": {"customColors": ["#1e1e2e", "#89b4fa"]}}));
    assert_eq!(arr.theme().static_custom(), vec!["#1e1e2e".to_string(), "#89b4fa".to_string()]);

    let csv = Config::from_root(json!({"theme": {"customColors": "#1e1e2e, #89b4fa , #f5c2e7"}}));
    assert_eq!(
        csv.theme().static_custom(),
        vec!["#1e1e2e".to_string(), "#89b4fa".to_string(), "#f5c2e7".to_string()],
    );
}

#[test]
fn fill_mode_per_output() {
    let defaults = Config::from_root(json!({}));
    assert_eq!(defaults.display().fill_mode_for("DP-1"), "fill");
    assert!(!defaults.display().fill_overrides_active());

    let configured = Config::from_root(json!({
        "display": {"fillMode": "fill", "fillModes": {"DP-2": "fit", "DP-3": "bogus"}}
    }));
    assert_eq!(configured.display().fill_mode_for("DP-2"), "fit");
    assert_eq!(configured.display().fill_mode_for("DP-3"), "fill");
    assert_eq!(configured.display().fill_mode_for("DP-1"), "fill");
    assert_eq!(configured.display().fill_override_for("DP-2").as_deref(), Some("fit"));
    assert_eq!(configured.display().fill_override_for("DP-3"), None);
    assert_eq!(configured.display().fill_override_for("DP-1"), None);
    assert!(configured.display().fill_overrides_active());

    let same = Config::from_root(json!({
        "display": {"fillMode": "fit", "fillModes": {"DP-2": "fit"}}
    }));
    assert_eq!(same.display().fill_mode_for("DP-2"), "fit");
    assert!(!same.display().fill_overrides_active());
}

#[test]
fn output_locks_default_off() {
    let defaults = Config::from_root(json!({}));
    assert!(!defaults.display().output_locked("DP-1"));
    assert!(defaults.display().locked_outputs().is_empty());

    let configured = Config::from_root(json!({
        "display": {"outputLocks": {"DP-1": true, "DP-2": false, "stale": "yes"}}
    }));
    assert!(configured.display().output_locked("DP-1"));
    assert!(!configured.display().output_locked("DP-2"));
    assert!(!configured.display().output_locked("stale"));
    assert_eq!(configured.display().locked_outputs(), ["DP-1"]);
}

#[test]
fn vitals_getters() {
    let defaults = Config::from_root(json!({}));
    assert!(defaults.vitals_enabled());
    assert_eq!(defaults.vitals_interval_mins(), 10);

    let low = Config::from_root(json!({"vitals": {"enabled": false, "intervalMins": 0.2}}));
    assert!(!low.vitals_enabled());
    assert_eq!(low.vitals_interval_mins(), 1);

    let high = Config::from_root(json!({"vitals": {"intervalMins": 30}}));
    assert!(high.vitals_enabled());
    assert_eq!(high.vitals_interval_mins(), 30);
}

#[test]
fn source_defaults() {
    let defaults = Config::from_root(json!({}));
    assert!(!defaults.bing_enabled());
    assert!(!defaults.unsplash_enabled());
    assert_eq!(defaults.bing_market(), "en-US");
    assert_eq!(defaults.unsplash_access_key(), "");
    assert!(defaults.source_enabled("wallhaven"));
    assert!(!defaults.source_enabled("bing"));
    assert!(!defaults.source_enabled("bogus"));
}

#[test]
fn plasma_lock_screen_policy() {
    let defaults = Config::from_root(json!({}));
    assert_eq!(defaults.plasma_lock_screen_mode(), "off");
    assert!(defaults.plasma_lock_screen_image().is_empty());
    assert!(!defaults.plasma_lock_screen_live());

    let follow = Config::from_root(json!({
        "plasma": {"lockScreen": {"mode": "follow", "dynamic": "LIVE"}}
    }));
    assert_eq!(follow.plasma_lock_screen_mode(), "follow");
    assert!(follow.plasma_lock_screen_live());

    let invalid = Config::from_root(json!({
        "plasma": {"lockScreen": {"mode": "replace-everything", "dynamic": "yes"}}
    }));
    assert_eq!(invalid.plasma_lock_screen_mode(), "off");
    assert!(!invalid.plasma_lock_screen_live());
}

#[test]
fn source_overrides() {
    let cfg = Config::from_root(json!({
        "sources": {
            "bing": {"enabled": true, "market": "en-GB"},
            "unsplash": {"enabled": true, "accessKey": "acc"}
        }
    }));
    assert!(cfg.bing_enabled() && cfg.source_enabled("bing"));
    assert_eq!(cfg.bing_market(), "en-GB");
    assert!(cfg.source_enabled("unsplash"));
    assert_eq!(cfg.unsplash_access_key(), "acc");
}

#[test]
fn dotted_override_clones() {
    let source = Config::from_root(json!({"theme": {"engine": "skwd-iris"}}));
    let preview = source.with_override("theme.engine", json!("wallust"));
    assert_eq!(source.theme().engine(), "skwd-iris");
    assert_eq!(preview.theme().engine(), "wallust");
}

#[test]
fn feature_facades_preserve_legacy_aliases_and_serialized_root() {
    let root = json!({
        "theme": {"backend": "noctalia"},
        "display": {"fillMode": "fit", "outputLocks": {"DP-2": true}},
        "paper": {"engine": "awww", "videoEngine": "tinier"},
        "transition": {"enabled": true, "shader": "fade", "durationMs": 750},
        "weRender": {"scaling": "center", "fps": 48},
        "wallpaperMute": false,
        "wallpaperVolume": 35,
        "futureFeature": {"version": 7, "enabled": true}
    });
    let config = Config::from_root(root.clone());

    assert_eq!(config.theme().backend(), "noctalia");
    assert_eq!(config.theme().policy(), "wallpaper");
    assert_eq!(config.theme().authority(), "noctalia");
    assert_eq!(config.theme().engine(), "skwd-iris");
    assert_eq!(config.display().fill_mode_for("DP-2"), "fit");
    assert!(config.display().output_locked("DP-2"));
    assert_eq!(config.renderer().engine(), "awww");
    assert_eq!(config.renderer().video_engine(), "tinier");
    assert_eq!(config.renderer().we_scene_fill_mode(), "center");
    assert_eq!(config.renderer().we_fps(), 48);
    assert!(!config.renderer().mute());
    assert_eq!(config.renderer().volume(), 35);
    assert!(config.transition().active());
    assert_eq!(config.transition().shader(), "fade");
    assert_eq!(config.transition().duration_ms(), 750);
    assert_eq!(serde_json::to_string(&config.root).unwrap(), serde_json::to_string(&root).unwrap());
}

#[test]
fn scoped_override_preserves_other_feature_serialization() {
    let source = Config::from_root(json!({
        "theme": {"mode": "dark"},
        "display": {"fillMode": "span"},
        "futureFeature": ["retained"]
    }));
    let preview = source.with_override("theme.mode", json!("light"));

    assert_eq!(source.theme().mode(), "dark");
    assert_eq!(preview.theme().mode(), "light");
    assert_eq!(preview.display().fill_mode(), "span");
    assert_eq!(preview.root["futureFeature"], json!(["retained"]));
    assert_eq!(source.root["theme"]["mode"], "dark");
}

#[test]
fn random_interval_floors() {
    let zero = Config::from_root(json!({"general": {"randomInterval": 0}}));
    assert_eq!(zero.random_interval(), 300);
    let tiny = Config::from_root(json!({"general": {"randomInterval": 3}}));
    assert_eq!(tiny.random_interval(), 10);
    let ok = Config::from_root(json!({"general": {"randomInterval": 600}}));
    assert_eq!(ok.random_interval(), 600);
    let missing = Config::from_root(json!({}));
    assert_eq!(missing.random_interval(), 300);
}

#[test]
fn rotate_needs_toggle() {
    let missing = Config::from_root(json!({}));
    assert!(!missing.random_rotate(),);
    let armed = Config::from_root(json!({"general": {"randomRotate": true}}));
    assert!(armed.random_rotate());
    let off = Config::from_root(json!({"general": {"randomRotate": false}}));
    assert!(!off.random_rotate());
}

#[test]
fn steam_root_we_dir() {
    let suffix = std::path::Path::new("steamapps/workshop/content/431960");

    let def = Config::from_root(json!({}));
    assert_eq!(def.steam_install_root().join(suffix), def.we_dir(),);

    let custom_steam = Config::from_root(json!({"paths": {"steam": "/mnt/steam"}}));
    assert_eq!(custom_steam.steam_install_root().join(suffix), custom_steam.we_dir());
    assert_eq!(custom_steam.steam_install_root(), std::path::Path::new("/mnt/steam"));

    let canonical_ws = Config::from_root(
        json!({"paths": {"steamWorkshop": "/data/steamapps/workshop/content/431960"}}),
    );
    assert_eq!(canonical_ws.steam_install_root().join(suffix), canonical_ws.we_dir(),);
}

#[test]
fn random_types_includes() {
    let cfg = Config::from_root(json!({"general": {
        "randomIncludeStatic": true, "randomIncludeVideo": false, "randomIncludeWE": true
    }}));
    assert_eq!(cfg.random_types(), vec!["static", "we"]);
    let defaults = Config::from_root(json!({}));
    assert_eq!(defaults.random_types(), vec!["static", "video", "we"]);
    assert!(defaults.random_favourites_only());
    assert!(
        !Config::from_root(json!({
            "general": {"randomIncludeFavourites": false}
        }))
        .random_favourites_only()
    );
}

#[test]
fn optimize_assets_getters() {
    let defaults = Config::from_root(json!({}));
    assert!(!defaults.image_auto_optimize());
    assert!(!defaults.image_auto_delete_trash());
    assert_eq!(defaults.image_optimize_preset(), "balanced");
    assert_eq!(defaults.image_optimize_resolution(), "2k");
    assert_eq!(defaults.image_trash_days(), 7);
    assert_eq!(defaults.max_thumb_jobs(), 16);
    assert_eq!(defaults.we_assets_dir(), "");

    let configured = Config::from_root(json!({
        "performance": {
            "autoOptimizeImages": true,
            "autoDeleteImageTrash": true,
            "imageOptimizePreset": "quality",
            "imageOptimizeResolution": "4k",
            "imageTrashDays": 30,
            "maxThumbJobs": 5
        },
        "paths": { "steamWeAssets": "/opt/wallpaper_engine/assets" }
    }));
    assert!(configured.image_auto_optimize());
    assert!(configured.image_auto_delete_trash());
    assert_eq!(configured.image_optimize_preset(), "quality");
    assert_eq!(configured.image_optimize_resolution(), "4k");
    assert_eq!(configured.image_trash_days(), 30);
    assert_eq!(configured.max_thumb_jobs(), 5);
    assert_eq!(configured.we_assets_dir(), "/opt/wallpaper_engine/assets");
}

#[test]
fn video_engine_getters() {
    let defaults = Config::from_root(json!({}));
    assert_eq!(defaults.renderer().video_engine(), "vulkan");
    assert!(defaults.renderer().video_multi_process());
    let gl = Config::from_root(json!({"paper": {"videoEngine": "gl"}}));
    assert_eq!(gl.renderer().video_engine(), "vulkan");
    let retired = Config::from_root(json!({"paper": {"videoEngine": "tiny"}}));
    assert_eq!(retired.renderer().video_engine(), "vulkan");
    let tinier = Config::from_root(json!({"paper": {"videoEngine": "tinier"}}));
    assert_eq!(tinier.renderer().video_engine(), "tinier");
    let compatibility = Config::from_root(json!({"paper": {"videoMultiProcess": false}}));
    assert!(!compatibility.renderer().video_multi_process());
    let vk_path = Config::from_root(json!({"paths": {"paperVkBin": "/opt/skwd-wall-vk"}}));
    assert_eq!(vk_path.renderer().vk_bin(), "/opt/skwd-wall-vk");
    let canonical = Config::from_root(json!({"paths": {"paperBin": "/opt/skwd-paper"}}));
    assert_eq!(canonical.renderer().paper_bin(), "/opt/skwd-paper");
}

#[test]
fn engine_fill_getters() {
    let defaults = Config::from_root(json!({}));
    assert_eq!(defaults.renderer().engine(), "skwd-paper");
    assert_eq!(defaults.display().fill_mode(), "fill");
    let cfg =
        Config::from_root(json!({"paper": {"engine": "awww"}, "display": {"fillMode": "fit"}}));
    assert_eq!(cfg.renderer().engine(), "awww");
    assert_eq!(cfg.display().fill_mode(), "fit");
    for retired in ["noctalia", "dms", "unknown"] {
        let legacy = Config::from_root(json!({"paper": {"engine": retired}}));
        assert_eq!(legacy.renderer().engine(), "skwd-paper");
    }
}

#[test]
fn transition_getters() {
    let defaults = Config::from_root(json!({}));
    assert!(defaults.transition().enabled());
    assert!(defaults.transition().active());
    assert_eq!(defaults.transition().shader(), "random");
    assert_eq!(defaults.transition().duration_ms(), 600);
    let cfg = Config::from_root(json!({"transition": {
        "enabled": false, "shader": "fade", "durationMs": 800.0
    }}));
    assert!(!cfg.transition().enabled());
    assert!(!cfg.transition().active());
    assert_eq!(cfg.transition().shader(), "fade");
    assert_eq!(cfg.transition().duration_ms(), 800);
    let clamp = Config::from_root(json!({"transition": {"durationMs": 50000.0}}));
    assert_eq!(clamp.transition().duration_ms(), 10000);

    let scopes = Config::from_root(json!({"transition": {
        "sandScope": "primary",
        "shaderScopes": {"fade": "primary", "sand-donut": "all", "glitch": "invalid"}
    }}));
    assert_eq!(scopes.transition().scope("fade"), "primary");
    assert_eq!(scopes.transition().scope("sand-donut"), "all");
    assert_eq!(scopes.transition().scope("sand-helix"), "primary");
    assert_eq!(scopes.transition().scope("glitch"), "all");

    let performance = Config::from_root(json!({"paper": {"performanceMode": true}}));
    assert!(performance.transition().enabled());
    assert!(!performance.transition().active());
}

#[test]
fn awww_arg_coercion() {
    let defaults = Config::from_root(json!({}));
    assert_eq!(defaults.renderer().awww_filter(), "Lanczos3");
    assert_eq!(defaults.renderer().awww_arg("transitionType"), None);
    let cfg = Config::from_root(json!({"paper": {"awww": {
        "filter": "Nearest", "transitionType": "wipe", "transitionFps": 60, "invertY": true
    }}}));
    assert_eq!(cfg.renderer().awww_filter(), "Nearest");
    assert_eq!(cfg.renderer().awww_arg("transitionType").as_deref(), Some("wipe"));
    assert_eq!(cfg.renderer().awww_arg("transitionFps").as_deref(), Some("60"));
    assert_eq!(cfg.renderer().awww_arg("invertY").as_deref(), Some("true"));
    let floats = Config::from_root(json!({"paper": {"awww": {
        "transitionFps": 144.0, "transitionStep": 90.0, "transitionAngle": 22.5
    }}}));
    assert_eq!(floats.renderer().awww_arg("transitionFps").as_deref(), Some("144"),);
    assert_eq!(floats.renderer().awww_arg("transitionStep").as_deref(), Some("90"));
    assert_eq!(floats.renderer().awww_arg("transitionAngle").as_deref(), Some("22.5"),);
    assert_eq!(
        cfg.renderer().awww_arg("transitionType"),
        cfg.renderer().awww_arg("transitionType")
    );
    let empty = Config::from_root(json!({"paper": {"awww": {"transitionPos": ""}}}));
    assert_eq!(empty.renderer().awww_arg("transitionPos"), None);
}

#[test]
fn matugen_getters() {
    let defaults = Config::from_root(json!({}));
    assert_eq!(defaults.theme().matugen_scheme(), "scheme-fidelity");
    assert_eq!(defaults.theme().matugen_mode(), "dark");
    assert_eq!(defaults.theme().matugen_color_index(), 0);
    assert_eq!(defaults.theme().matugen_contrast(), None);
    let cfg = Config::from_root(json!({"matugen": {
        "schemeType": "scheme-vibrant", "mode": "light", "colorIndex": 2, "contrast": 0.5
    }}));
    assert_eq!(cfg.theme().matugen_scheme(), "scheme-vibrant");
    assert_eq!(cfg.theme().matugen_mode(), "light");
    assert_eq!(cfg.theme().matugen_color_index(), 2);
    assert_eq!(cfg.theme().matugen_contrast(), Some(0.5));
    assert_eq!(
        Config::from_root(json!({"matugen": {"colorIndex": 9}})).theme().matugen_color_index(),
        3,
    );

    let inherited = Config::from_root(json!({"matugen": {"mode": "light"}}));
    assert_eq!(inherited.theme().mode(), "light");
    let explicit = Config::from_root(json!({
        "theme": {"mode": "dark"}, "matugen": {"mode": "light"}
    }));
    assert_eq!(explicit.theme().mode(), "dark");
}

#[test]
fn we_render_getters() {
    let defaults = Config::from_root(json!({}));
    assert_eq!(defaults.renderer().we_scaling(), "default");
    assert_eq!(defaults.renderer().we_scene_fill_mode(), "fill");
    assert_eq!(defaults.renderer().we_fps(), 30);
    assert!(!defaults.renderer().we_disable_particles());
    let cfg = Config::from_root(json!({
        "display": {"fillMode": "fit"},
        "weRender": {"scaling": "center", "fps": 30, "disableParticles": true}
    }));
    assert_eq!(cfg.renderer().we_scaling(), "center");
    assert_eq!(cfg.renderer().we_scene_fill_mode(), "center");
    assert_eq!(cfg.renderer().we_fps(), 30);
    assert!(cfg.renderer().we_disable_particles());
    assert_eq!(
        Config::from_root(json!({
            "display": {"fillMode": "fit"}, "weRender": {"scaling": "default"}
        }))
        .renderer()
        .we_scene_fill_mode(),
        "fit"
    );
    assert_eq!(
        Config::from_root(json!({
            "display": {"fillMode": "span"}, "weRender": {"scaling": "invalid"}
        }))
        .renderer()
        .we_scene_fill_mode(),
        "span"
    );
    let float_fps = Config::from_root(json!({"weRender": {"fps": 30.0}}));
    assert_eq!(float_fps.renderer().we_fps(), 30);
    let one = Config::from_root(json!({"weRender": {"fps": 1.0}}));
    assert_eq!(one.renderer().we_fps(), 1);
}

#[test]
fn steam_wallhaven_getters() {
    let defaults = Config::from_root(json!({}));
    assert_eq!(defaults.steam_backend(), "steam");
    assert_eq!(defaults.steam_username(), "");
    assert_eq!(defaults.steam_api_key(), "");
    assert_eq!(defaults.wallhaven_api_key(), "");
    let cfg = Config::from_root(json!({
        "steam": {"backend": "steamcmd", "username": "me", "apiKey": "sk"},
        "wallhaven": {"apiKey": "wh"}
    }));
    assert_eq!(cfg.steam_backend(), "steamcmd");
    assert_eq!(cfg.steam_username(), "me");
    assert_eq!(cfg.steam_api_key(), "sk");
    assert_eq!(cfg.wallhaven_api_key(), "wh");
}

#[test]
fn flags_default_true() {
    let defaults = Config::from_root(json!({}));
    assert!(defaults.theme().matugen_enabled());
    assert!(defaults.notify_on_change());
    assert!(defaults.restore_on_startup());
    assert!(defaults.steam_enabled());
    let off = Config::from_root(json!({
        "features": {"matugen": false, "steam": false},
        "general": {"notifyOnWallpaperChange": false}, "restoreOnStartup": false
    }));
    assert!(!off.theme().matugen_enabled());
    assert!(!off.notify_on_change());
    assert!(!off.restore_on_startup());
    assert!(!off.steam_enabled());
}

#[test]
fn niri_backdrop_getters() {
    let defaults = Config::from_root(json!({}));
    assert!(!defaults.niri_overview_backdrop());
    assert!(defaults.niri_backdrop_blur_enabled());
    assert!(defaults.niri_backdrop_follow_wallpaper());
    assert_eq!(defaults.niri_backdrop_blur(), 20.0);
    assert_eq!(defaults.niri_backdrop_dim(), 0);
    let cfg = Config::from_root(json!({"niri": {
        "overviewBackdrop": true, "overviewBackdropBlurEnabled": false,
        "overviewBackdropBlur": 35.0, "backdropFollowWallpaper": false, "backdropDim": 40.0
    }}));
    assert!(cfg.niri_overview_backdrop());
    assert!(!cfg.niri_backdrop_blur_enabled());
    assert_eq!(cfg.niri_backdrop_blur(), 35.0);
    assert!(!cfg.niri_backdrop_follow_wallpaper());
    assert_eq!(cfg.niri_backdrop_dim(), 40);
    assert_eq!(Config::from_root(json!({"niri": {"backdropDim": 999.0}})).niri_backdrop_dim(), 100,);
    assert!(!defaults.niri_backdrop_auto_theme());
    assert_eq!(defaults.niri_backdrop_theme(), "");
    let themed =
        Config::from_root(json!({"niri": {"backdropAutoTheme": true, "backdropTheme": "Nord"}}));
    assert!(themed.niri_backdrop_auto_theme());
    assert_eq!(themed.niri_backdrop_theme(), "Nord");
}

#[test]
fn audio_getters() {
    let defaults = Config::from_root(json!({}));
    assert!(defaults.renderer().mute());
    assert_eq!(defaults.renderer().volume(), 100);
    let cfg = Config::from_root(json!({"wallpaperMute": false, "wallpaperVolume": 60}));
    assert!(!cfg.renderer().mute());
    assert_eq!(cfg.renderer().volume(), 60);
    assert_eq!(Config::from_root(json!({"wallpaperVolume": 500})).renderer().volume(), 100,);
}

#[test]
fn integration_live_preview_defaults_on() {
    let cfg = Config::from_root(json!({
        "integrations": [
            { "template": "a", "output": "/a" },
            { "template": "b", "output": "/b", "livePreview": false },
            { "template": "c", "output": "/c", "livePreview": true },
        ]
    }));
    let ints = cfg.theme().integrations();
    assert!(ints[0].live_preview);
    assert!(!ints[1].live_preview);
    assert!(ints[2].live_preview);
}

#[test]
fn kde_live_preview_off() {
    let config = Config::from_root(json!({
        "integrations": [
            { "name": "kde", "template": "a", "output": "/a" },
            { "name": "plasma", "template": "b", "output": "/b" },
            { "name": "kde", "template": "c", "output": "/c", "livePreview": true }
        ]
    }));
    let integrations = config.theme().integrations();
    assert!(!integrations[0].live_preview);
    assert!(!integrations[1].live_preview);
    assert!(integrations[2].live_preview);
}

#[test]
fn read_root_garbage() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(read_root(&dir.path().join("nope.json")), Value::Null);
    for garbage in ["", "{\"paper\":", "not json at all", "{\"a\": tru"] {
        let path = dir.path().join("bad.json");
        std::fs::write(&path, garbage).unwrap();
        assert_eq!(read_root(&path), Value::Null);
    }
}

#[test]
fn getters_non_object_root() {
    for root in [Value::Null, json!([]), json!("oops"), json!(42), json!(true)] {
        let cfg = Config::from_root(root.clone());
        assert!(cfg.wallpaper_dir().ends_with("Pictures/Wallpapers"));
        assert_eq!(cfg.video_dir(), cfg.wallpaper_dir());
        assert!(!cfg.cache_dir().is_empty());
        assert_eq!(cfg.display().fill_mode(), "fill");
        assert_eq!(cfg.renderer().engine(), "skwd-paper");
        assert_eq!(cfg.display().fill_color(), "000000ff");
        assert!(cfg.transition().enabled());
        assert_eq!(cfg.transition().shader(), "random");
        assert_eq!(cfg.transition().duration_ms(), 600);
        assert_eq!(cfg.renderer().video_engine(), "vulkan");
        assert!(cfg.renderer().mute());
        assert_eq!(cfg.renderer().volume(), 100);
        assert_eq!(cfg.theme().backend(), "skwd-iris");
        assert_eq!(cfg.theme().mode(), "dark");
        assert_eq!(cfg.random_interval(), 300);
        assert_eq!(cfg.random_types(), vec!["static", "video", "we"]);
        assert_eq!(cfg.latitude(), 0.0);
        assert!(cfg.schedule_entries().is_empty());
        assert!(!cfg.schedule_enabled());
        assert!(cfg.schedule_apply_on_start(),);
        assert!(cfg.playlist_lists().is_empty());
        assert!(cfg.playlist_assign().is_empty());
        assert!(cfg.we_dir().ends_with("steamapps/workshop/content/431960"));
        assert_eq!(cfg.renderer().we_fps(), 30);
        assert!(!cfg.renderer().we_disable_particles());
        assert!(cfg.post_processing().is_empty());
        assert!(cfg.theme().integrations().is_empty());
        assert!(cfg.theme().native_templates().is_empty());
        assert_eq!(cfg.theme().matugen_contrast(), None);
        assert!(cfg.restore_on_startup());
        assert!(!cfg.pick_only_mode());
        assert_eq!(cfg.steam_backend(), "steam");
        assert!(cfg.theme().wallust_palette().is_none());
        assert!(cfg.theme().default_matugen_config().is_none());
        assert!(cfg.theme().external_matugen_command().is_none());
    }
}

#[test]
fn corrupt_config_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    std::fs::write(&path, "{\"transition\": {\"shader\": ").unwrap();
    let (cfg, _) = Config::load_path_if_changed(&path, None).expect("mtime readable");
    assert_eq!(cfg.display().fill_mode(), "fill");
    assert_eq!(cfg.transition().shader(), "random");
    assert!(Config::load_path_if_changed(&dir.path().join("absent.json"), None).is_none(),);
}

#[test]
fn history_defaults() {
    let cfg = Config::from_root(serde_json::json!({}));
    assert!(cfg.history_enabled());
    assert_eq!(cfg.history_depth(), 50);
}

#[test]
fn library_polling_defaults_and_bounds() {
    let cfg = Config::from_root(serde_json::json!({}));
    assert!(!cfg.library_polling_fallback());
    assert_eq!(cfg.library_polling_interval_seconds(), 60);
    let low = Config::from_root(serde_json::json!({
        "library": { "pollingFallback": true, "pollingIntervalSeconds": 1 }
    }));
    assert!(low.library_polling_fallback());
    assert_eq!(low.library_polling_interval_seconds(), 15);
    let high = Config::from_root(serde_json::json!({
        "library": { "pollingIntervalSeconds": 9000 }
    }));
    assert_eq!(high.library_polling_interval_seconds(), 3600);
}

#[test]
fn history_clamps() {
    let cfg = Config::from_root(serde_json::json!({"history": {"enabled": false, "depth": 5000}}));
    assert!(!cfg.history_enabled());
    assert_eq!(cfg.history_depth(), 1000);
    let cfg = Config::from_root(serde_json::json!({"history": {"depth": 0}}));
    assert_eq!(cfg.history_depth(), 1);
}

#[test]
fn load_skips_same_mtime() {
    let dir = std::env::temp_dir().join(format!("skwd-cfg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.json");
    std::fs::write(&path, r#"{"wallpaper":{"mute":true}}"#).unwrap();

    let (cfg, mtime) = Config::load_path_if_changed(&path, None).expect("first load");
    assert!(cfg.renderer().mute());

    assert!(Config::load_path_if_changed(&path, Some(mtime)).is_none(),);

    let earlier = mtime.checked_sub(std::time::Duration::from_secs(1)).unwrap();
    let (reloaded, remtime) =
        Config::load_path_if_changed(&path, Some(earlier)).expect("mtime mismatch must reload");
    assert_eq!(remtime, mtime);
    assert!(reloaded.renderer().mute());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn reload_retries_invalid_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    std::fs::write(&path, r#"{"pickOnlyMode":false}"#).unwrap();
    let (_, original_mtime) =
        Config::load_valid_path_if_changed(&path, None).expect("initial config");
    let changed_mtime = original_mtime + std::time::Duration::from_secs(1);

    std::fs::write(&path, "").unwrap();
    std::fs::File::options().write(true).open(&path).unwrap().set_modified(changed_mtime).unwrap();
    assert!(Config::load_valid_path_if_changed(&path, Some(original_mtime)).is_none());

    std::fs::write(&path, r#"{"pickOnlyMode":true}"#).unwrap();
    std::fs::File::options().write(true).open(&path).unwrap().set_modified(changed_mtime).unwrap();
    let (reloaded, _) =
        Config::load_valid_path_if_changed(&path, Some(original_mtime)).expect("valid rewrite");
    assert!(reloaded.pick_only_mode());
}

#[test]
fn steam_root_layouts() {
    use std::path::{Path, PathBuf};
    let hit = |target: &'static str| {
        move |path: &Path| path == Path::new("/h").join(target).join("steamapps")
    };
    assert_eq!(
        detect_steam_root("/h", hit(".local/share/Steam")),
        PathBuf::from("/h/.local/share/Steam")
    );
    assert_eq!(detect_steam_root("/h", hit(".steam/steam")), PathBuf::from("/h/.steam/steam"));
    assert_eq!(
        detect_steam_root("/h", hit(".var/app/com.valvesoftware.Steam/.local/share/Steam")),
        PathBuf::from("/h/.var/app/com.valvesoftware.Steam/.local/share/Steam")
    );
    assert_eq!(detect_steam_root("/h", |_| false), PathBuf::from("/h/.local/share/Steam"));
    let both = |path: &Path| {
        path == Path::new("/h/.local/share/Steam/steamapps")
            || path == Path::new("/h/.steam/steam/steamapps")
    };
    assert_eq!(detect_steam_root("/h", both), PathBuf::from("/h/.local/share/Steam"));
}

use crate::xorshift64 as xs;

const FUZZ_KEYS: &[&str] = &[
    "enabled",
    "durationMs",
    "shader",
    "rules",
    "wallpaper",
    "engine",
    "codec",
    "maxHeight",
    "latitude",
    "volume",
    "interval",
    "paths",
    "when",
    "set",
    "🌊",
];

fn fuzz_value(seed: &mut u64, depth: u32) -> serde_json::Value {
    match xs(seed) % if depth == 0 { 6 } else { 8 } {
        0 => serde_json::json!(null),
        1 => serde_json::json!((xs(seed) as i64).wrapping_mul(7)),
        2 => serde_json::json!(f64::from_bits(xs(seed)).clamp(-1e308, 1e308)),
        3 => serde_json::json!(xs(seed).is_multiple_of(2)),
        4 => serde_json::json!(""),
        5 => serde_json::json!(["🌊", -0.0, {"x": null}, 9e99]),
        6 => {
            let mut map = serde_json::Map::new();
            for _ in 0..(xs(seed) % 5) {
                map.insert(
                    FUZZ_KEYS[(xs(seed) % FUZZ_KEYS.len() as u64) as usize].to_string(),
                    fuzz_value(seed, depth - 1),
                );
            }
            serde_json::Value::Object(map)
        }
        _ => serde_json::json!([fuzz_value(seed, depth - 1)]),
    }
}

#[test]
fn getters_survive_fuzz() {
    const TOP: &[&str] = &[
        "general",
        "transition",
        "schedule",
        "paths",
        "display",
        "paper",
        "videoOptimize",
        "history",
        "theme",
        "sources",
        "features",
        "postProcessing",
        "workspace",
        "effects",
        "analysis",
        "niri",
    ];
    let mut seed = 0x5eed_0004u64;
    for _ in 0..600 {
        let mut root = serde_json::Map::new();
        for _ in 0..(xs(&mut seed) % 8) {
            root.insert(
                TOP[(xs(&mut seed) % TOP.len() as u64) as usize].to_string(),
                fuzz_value(&mut seed, 3),
            );
        }
        let cfg = Config::from_root(serde_json::Value::Object(root));
        let _ = (
            cfg.renderer().engine(),
            cfg.display().fill_mode(),
            cfg.display().fill_color(),
            cfg.wallpaper_dir(),
            cfg.video_dir(),
        );
        let _ = (
            cfg.cache_dir(),
            cfg.renderer().paper_bin(),
            cfg.renderer().still_bin(),
            cfg.renderer().vk_bin(),
            cfg.renderer().video_engine(),
        );
        let _ =
            (cfg.transition().enabled(), cfg.transition().shader(), cfg.transition().duration_ms());
        let _ = (cfg.history_enabled(), cfg.history_depth(), cfg.pick_only_mode());
        let _ = (cfg.renderer().mute(), cfg.renderer().volume());
        let _ = (cfg.random_interval(), cfg.random_rotate(), cfg.random_types());
        let _ = (cfg.schedule_enabled(), cfg.schedule_solar(), cfg.schedule_rules());
        let _ =
            (cfg.latitude(), cfg.longitude(), cfg.locale(), cfg.we_dir(), cfg.steam_install_root());
        let _ = (cfg.post_processing().len(), cfg.theme().backend(), cfg.renderer().awww_filter());
        let _ = cfg.workspace_enabled();
    }
}

use proptest::prelude::*;

fn arb_json() -> impl Strategy<Value = serde_json::Value> {
    let leaf = prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::from),
        any::<i64>().prop_map(serde_json::Value::from),
        (-1.0e7f64..1.0e7).prop_map(|num| serde_json::json!(num)),
        "[a-zA-Z0-9:./ _-]{0,24}".prop_map(serde_json::Value::from),
    ];
    leaf.prop_recursive(3, 24, 5, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(serde_json::Value::from),
            prop::collection::hash_map("[a-zA-Z]{1,12}", inner, 0..6)
                .prop_map(|map| serde_json::Value::Object(map.into_iter().collect())),
        ]
    })
}

proptest! {
    #[test]
    fn getters_arbitrary_json(root in arb_json()) {
        let cfg = Config::from_root(root);
        prop_assert!(cfg.theme().matugen_color_index() <= 3);
        prop_assert!(cfg.renderer().volume() <= 100);
        let interval = cfg.random_interval();
        prop_assert!(interval == 300 || interval >= 10, "interval={interval}");
        let _ = (cfg.cache_dir(), cfg.theme().backend(), cfg.renderer().awww_filter());
        let _ = (cfg.post_processing().len(), cfg.workspace_enabled());
    }
}

#[test]
fn theme_targets_valid_unique() {
    let cfg = Config::from_root(serde_json::json!({
        "theme": {"targets": ["caelestia", "dms", "caelestia", "unknown", 12, "end4"]}
    }));
    assert_eq!(cfg.theme().targets(), ["caelestia", "dms", "end4"]);
    assert!(Config::from_root(serde_json::json!({})).theme().targets().is_empty());
}
