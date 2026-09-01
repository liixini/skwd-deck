#![cfg(test)]

use super::*;

fn info(name: &str) -> OutputInfo {
    OutputInfo {
        name: name.to_string(),
        make: String::new(),
        model: String::new(),
        width: 1920,
        height: 1080,
        refresh_mhz: 60000,
        scale: 1,
        rotated: false,
    }
}

#[test]
fn rotated_swaps_dims() {
    let flat = info("DP-1");
    assert_eq!(flat.logical_size(), (1920, 1080));
    let portrait = OutputInfo { rotated: true, ..info("DP-2") };
    assert_eq!(portrait.logical_size(), (1080, 1920));
}

#[test]
fn refresh_caps_limit() {
    assert_eq!(effective_fps(165, 164_835), 165);
    assert_eq!(effective_fps(165, 59_997), 60);
    assert_eq!(effective_fps(165, 144_001), 144);
    assert_eq!(effective_fps(30, 164_835), 30);
    assert_eq!(effective_fps(165, 0), 165);
}

#[test]
fn target_cap_fastest() {
    let outputs = vec![
        OutputInfo { refresh_mhz: 164_835, ..info("DP-1") },
        OutputInfo { refresh_mhz: 59_997, ..info("DP-2") },
        OutputInfo { refresh_mhz: 144_001, ..info("DP-3") },
    ];
    assert_eq!(target_fps(165, "DP-2", &outputs), 60);
    assert_eq!(target_fps(165, "DP-2,DP-3", &outputs), 144);
    assert_eq!(target_fps(165, "*", &outputs), 165);
    assert_eq!(target_fps(30, "*", &outputs), 30);
    assert_eq!(target_fps(165, "missing", &outputs), 165);
    assert_eq!(refresh_signature(&outputs), "DP-1:164835,DP-2:59997,DP-3:144001");
    assert_eq!(fps_map(165, &outputs), "DP-1=165;DP-2=60;DP-3=144");
}

#[test]
fn fake_output_spec_parsing() {
    let sized = parse_fake_output("DP-1:2560x1440").unwrap();
    assert_eq!((sized.name.as_str(), sized.width, sized.height), ("DP-1", 2560, 1440));
    let defaulted = parse_fake_output("HDMI-A-1").unwrap();
    assert_eq!(
        (defaulted.name.as_str(), defaulted.width, defaulted.height),
        ("HDMI-A-1", 1920, 1080)
    );
    assert_eq!(parse_fake_output("  eDP-1 : 800 x 600 ").unwrap().name, "eDP-1");
    assert!(parse_fake_output("").is_none());
    assert!(parse_fake_output("   ").is_none());
}

#[test]
fn enumerate_cache_rules() {
    let _guard = enum_exclusive();
    invalidate();

    assert!(enumerate_with(Vec::new).is_empty());

    let mut fetched = false;
    let first = enumerate_with(|| {
        fetched = true;
        vec![info("DP-1")]
    });
    assert!(fetched);
    assert_eq!(first[0].name, "DP-1");

    let mut refetched = false;
    let cached = enumerate_with(|| {
        refetched = true;
        vec![info("DP-2")]
    });
    assert!(!refetched);
    assert_eq!(cached[0].name, "DP-1");

    invalidate();
    let mut after_invalidate = false;
    let fresh = enumerate_with(|| {
        after_invalidate = true;
        vec![info("DP-2")]
    });
    assert!(after_invalidate);
    assert_eq!(fresh[0].name, "DP-2");

    invalidate();
}

#[test]
fn identity_prefers_monitor() {
    let named =
        OutputInfo { make: String::from("Dell"), model: String::from("U2723QE"), ..info("DP-1") };
    assert_eq!(named.identity(), "Dell U2723QE");
    assert_eq!(info("DP-3").identity(), "DP-3");
    let model_only = OutputInfo { model: String::from("XG27AQ"), ..info("DP-2") };
    assert_eq!(model_only.identity(), "XG27AQ");
}

#[test]
fn portrait_from_rotation() {
    assert!(!info("DP-1").portrait());
    assert!(OutputInfo { rotated: true, ..info("DP-2") }.portrait());
}

#[test]
fn identity_key_pairs_connector() {
    let named =
        OutputInfo { make: String::from("Dell"), model: String::from("U2723QE"), ..info("DP-1") };
    assert_eq!(named.identity_key(), "Dell U2723QE @ DP-1");
    let twin =
        OutputInfo { make: String::from("Dell"), model: String::from("U2723QE"), ..info("DP-2") };
    assert_ne!(named.identity_key(), twin.identity_key());
    assert_eq!(named.identity(), twin.identity());
    assert_eq!(info("DP-3").identity_key(), "DP-3");
}
