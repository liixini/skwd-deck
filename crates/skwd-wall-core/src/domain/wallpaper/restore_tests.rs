use super::*;

fn still(path: &str) -> Wallpaper {
    Wallpaper {
        kind: String::from(wall_proto::kind::STATIC),
        path: String::from(path),
        we_id: String::new(),
    }
}

fn target(
    output: &str,
    portrait: bool,
    policy: OutputPolicy,
    live: Option<Wallpaper>,
) -> OutputTargetState {
    OutputTargetState {
        output: String::from(output),
        portrait,
        policy,
        live: live.map(|wallpaper| wallpaper.assigned().to_string()),
    }
}

fn last(any: &str, landscape: &str, portrait: &str) -> LastApplied {
    LastApplied {
        any: (!any.is_empty()).then(|| still(any)),
        landscape: (!landscape.is_empty()).then(|| still(landscape)),
        portrait: (!portrait.is_empty()).then(|| still(portrait)),
    }
}

#[test]
fn pin_per_output() {
    let outputs = [
        target("DP-1", false, OutputPolicy::Pin(still("/a.png")), None),
        target("DP-2", true, OutputPolicy::Pin(still("/b.png")), None),
        target("DP-3", false, OutputPolicy::Pin(still("/c.png")), None),
    ];
    let resolved = resolve(&outputs, &last("/last.png", "", ""));
    assert_eq!(
        resolved
            .iter()
            .map(|row| (row.output.as_str(), row.wallpaper.path.as_str()))
            .collect::<Vec<_>>(),
        [("DP-1", "/a.png"), ("DP-2", "/b.png"), ("DP-3", "/c.png")]
    );
}

#[test]
fn follow_matches_orientation() {
    let outputs = [
        target("DP-1", false, OutputPolicy::FollowDimension, None),
        target("DP-2", true, OutputPolicy::FollowDimension, None),
    ];
    let resolved = resolve(&outputs, &last("/any.png", "/wide.png", "/tall.png"));
    assert_eq!(resolved[0].wallpaper.path, "/wide.png");
    assert_eq!(resolved[1].wallpaper.path, "/tall.png");
}

#[test]
fn follow_falls_back() {
    let outputs = [target("DP-2", true, OutputPolicy::FollowDimension, None)];
    let resolved = resolve(&outputs, &last("/any.png", "/wide.png", ""));
    assert_eq!(resolved[0].wallpaper.path, "/any.png");
}

#[test]
fn resolve_nothing_known() {
    let outputs = [target("DP-1", false, OutputPolicy::FollowDimension, None)];
    assert!(resolve(&outputs, &LastApplied::default()).is_empty());
}

#[test]
fn group_collapses_to_star() {
    let live = [String::from("DP-1"), String::from("DP-2"), String::from("DP-3")];
    let outputs = [
        target("DP-1", false, OutputPolicy::Pin(still("/a.png")), None),
        target("DP-2", true, OutputPolicy::Pin(still("/a.png")), None),
        target("DP-3", false, OutputPolicy::Pin(still("/a.png")), None),
    ];
    let groups = group(&resolve(&outputs, &LastApplied::default()), &live);
    assert_eq!(groups, [ApplyGroup { target: String::from("*"), wallpaper: still("/a.png") }]);
}

#[test]
fn group_partial_sharing() {
    let live = [String::from("DP-1"), String::from("DP-2"), String::from("DP-3")];
    let outputs = [
        target("DP-1", false, OutputPolicy::Pin(still("/a.png")), None),
        target("DP-2", true, OutputPolicy::Pin(still("/b.png")), None),
        target("DP-3", false, OutputPolicy::Pin(still("/a.png")), None),
    ];
    let groups = group(&resolve(&outputs, &LastApplied::default()), &live);
    assert_eq!(
        groups,
        [
            ApplyGroup { target: String::from("DP-1,DP-3"), wallpaper: still("/a.png") },
            ApplyGroup { target: String::from("DP-2"), wallpaper: still("/b.png") },
        ]
    );
}

#[test]
fn pending_all_current() {
    let live = [String::from("DP-1"), String::from("DP-2")];
    let outputs = [
        target("DP-1", false, OutputPolicy::Pin(still("/a.png")), Some(still("/a.png"))),
        target("DP-2", true, OutputPolicy::Pin(still("/b.png")), Some(still("/b.png"))),
    ];
    assert!(pending(&resolve(&outputs, &LastApplied::default()), &live).is_empty());
}

#[test]
fn stale_output_reapplies_group() {
    let live = [String::from("DP-1"), String::from("DP-2")];
    let outputs = [
        target("DP-1", false, OutputPolicy::Pin(still("/a.png")), Some(still("/a.png"))),
        target("DP-2", true, OutputPolicy::Pin(still("/b.png")), Some(still("/stale.png"))),
    ];
    let groups = pending(&resolve(&outputs, &LastApplied::default()), &live);
    assert_eq!(groups.len(), 2);
}

#[test]
fn record_tracks_orientation() {
    let mut last = LastApplied::default();
    last.record(&still("/wide.png"), false);
    last.record(&still("/tall.png"), true);
    assert_eq!(last.any.as_ref().unwrap().path, "/tall.png");
    assert_eq!(last.landscape.as_ref().unwrap().path, "/wide.png");
    assert_eq!(last.portrait.as_ref().unwrap().path, "/tall.png");
    last.record(&Wallpaper::default(), false);
    assert_eq!(last.any.as_ref().unwrap().path, "/tall.png");
}

#[test]
fn policy_wire_round_trip() {
    assert_eq!(OutputPolicy::FollowDimension.as_key(), "follow-dimension");
    assert_eq!(OutputPolicy::Pin(still("/a.png")).as_key(), "pin");
    assert_eq!(OutputPolicy::parse("follow", None), None);
    assert_eq!(
        OutputPolicy::parse("pin", Some(still("/a.png"))),
        Some(OutputPolicy::Pin(still("/a.png")))
    );
    assert_eq!(OutputPolicy::parse("pin", None), None);
    assert_eq!(OutputPolicy::parse("nonsense", None), None);
}

#[test]
fn pin_beats_dimension() {
    let outputs = [target("DP-2", true, OutputPolicy::Pin(still("/pinned.png")), None)];
    let resolved = resolve(&outputs, &last("/any.png", "/wide.png", "/tall.png"));
    assert_eq!(resolved[0].wallpaper.path, "/pinned.png");
}

#[test]
fn follow_nothing_to_apply() {
    let outputs = [
        target("DP-1", false, OutputPolicy::FollowDimension, None),
        target("DP-2", true, OutputPolicy::FollowDimension, None),
        target("DP-3", false, OutputPolicy::Pin(still("/pinned.png")), None),
    ];
    let resolved = resolve(&outputs, &LastApplied::default());
    assert_eq!(resolved.iter().map(|row| row.output.as_str()).collect::<Vec<_>>(), ["DP-3"]);
}

#[test]
fn follow_catches_up_stale() {
    let outputs = [target("DP-3", false, OutputPolicy::FollowDimension, Some(still("/stale.png")))];
    let resolved = resolve(&outputs, &last("/tall.png", "/newest-wide.png", "/tall.png"));
    assert_eq!(resolved[0].wallpaper.path, "/newest-wide.png");
    assert!(!resolved[0].already_applied);
}

#[test]
fn orientation_sets_isolated() {
    let outputs = [
        target("DP-1", false, OutputPolicy::FollowDimension, None),
        target("DP-2", true, OutputPolicy::FollowDimension, None),
    ];
    let resolved = resolve(&outputs, &last("/tall.png", "/wide.png", "/tall.png"));
    assert_eq!(resolved[0].wallpaper.path, "/wide.png");
    assert_eq!(resolved[1].wallpaper.path, "/tall.png");
}

#[test]
fn pending_cold_start() {
    let live = [String::from("DP-1"), String::from("DP-2")];
    let outputs = [
        target("DP-1", false, OutputPolicy::Pin(still("/a.png")), None),
        target("DP-2", true, OutputPolicy::Pin(still("/b.png")), None),
    ];
    let groups = pending(&resolve(&outputs, &LastApplied::default()), &live);
    assert_eq!(groups.len(), 2);
}
