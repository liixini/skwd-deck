#![cfg(test)]

use super::*;
use serde_json::json;

fn info(output: &str, idx: u64, name: Option<&str>, active: bool) -> WorkspaceInfo {
    WorkspaceInfo { output: output.to_string(), idx, name: name.map(str::to_string), active }
}

#[test]
fn parse_rule_required_fields() {
    assert!(parse_rule(&json!({ "workspace": "2" })).is_none());
    assert!(parse_rule(&json!({ "wallpaper": "static:a.jpg" })).is_none());
    assert!(parse_rule(&json!({ "wallpaper": "", "workspace": "2" })).is_none());
    let rule =
        parse_rule(&json!({ "output": "DP-1", "workspace": "2", "wallpaper": "static:a.jpg" }))
            .expect("valid rule");
    assert_eq!(
        (rule.output.as_str(), rule.matcher.as_str(), rule.wallpaper.as_str()),
        ("DP-1", "2", "static:a.jpg")
    );
}

#[test]
fn parse_rule_workspace_forms() {
    let numeric =
        parse_rule(&json!({ "workspace": 3, "wallpaper": "static:a.jpg" })).expect("numeric");
    assert_eq!(numeric.matcher, "3");
    let named =
        parse_rule(&json!({ "workspace": "code", "wallpaper": "video:b.mp4" })).expect("named");
    assert_eq!(named.matcher, "code");
}

#[test]
fn match_rule_scoping() {
    let rules = parse_rules(&[
        json!({ "output": "DP-1", "workspace": "2", "wallpaper": "a" }),
        json!({ "workspace": "code", "wallpaper": "b" }),
        json!({ "output": "DP-2", "workspace": "2", "wallpaper": "c" }),
    ]);
    assert_eq!(match_rule(&rules, &info("DP-1", 2, None, true)), Some("a"));
    assert_eq!(match_rule(&rules, &info("DP-2", 2, None, true)), Some("c"));
    assert_eq!(match_rule(&rules, &info("DP-9", 5, Some("code"), true)), Some("b"));
    assert_eq!(match_rule(&rules, &info("DP-1", 9, None, true)), None);
}

#[test]
fn named_beats_index() {
    let rules = parse_rules(&[json!({ "workspace": "code", "wallpaper": "named" })]);
    assert_eq!(match_rule(&rules, &info("DP-1", 2, Some("code"), true)), Some("named"));
    assert_eq!(match_rule(&rules, &info("DP-1", 2, Some("chat"), true)), None);
}

#[test]
fn topology_parses() {
    let event = json!({
        "WorkspacesChanged": { "workspaces": [
            { "id": 1, "idx": 1, "name": null, "output": "DP-1", "is_active": false },
            { "id": 2, "idx": 2, "name": "code", "output": "DP-1", "is_active": true }
        ]}
    });
    let topo = parse_topology(&event).expect("topology");
    assert_eq!(topo.len(), 2);
    assert!(topo[&2].active);
    assert_eq!(topo[&2].name.as_deref(), Some("code"));
    assert_eq!(topo[&1].output, "DP-1");
    assert!(parse_topology(&json!({ "WindowsChanged": {} })).is_none());
}

#[test]
fn activated_id() {
    assert_eq!(
        parse_activated(&json!({ "WorkspaceActivated": { "id": 7, "focused": true } })),
        Some(7)
    );
    assert_eq!(parse_activated(&json!({ "WorkspacesChanged": {} })), None);
}

fn topo(pairs: &[(u64, &str, u64, bool)]) -> HashMap<u64, WorkspaceInfo> {
    pairs.iter().map(|&(id, out, idx, act)| (id, info(out, idx, None, act))).collect()
}

#[test]
fn mark_active_scoped() {
    let mut workspaces = topo(&[(1, "DP-1", 1, true), (2, "DP-1", 2, false), (3, "DP-2", 1, true)]);
    let dir = mark_active(&mut workspaces, 2);
    assert!(!workspaces[&1].active);
    assert!(workspaces[&2].active);
    assert!(workspaces[&3].active);
    assert_eq!(dir, Some(("DP-1".to_string(), "up")));
}

#[test]
fn mark_active_direction() {
    let mut workspaces =
        topo(&[(1, "DP-1", 1, false), (2, "DP-1", 2, false), (3, "DP-1", 3, true)]);
    assert_eq!(mark_active(&mut workspaces, 1), Some(("DP-1".to_string(), "down")));
    assert_eq!(mark_active(&mut workspaces, 1), None);
    let mut orphan = topo(&[(5, "DP-2", 1, false)]);
    assert_eq!(mark_active(&mut orphan, 5), None);
}

#[test]
fn preload_paths_pins() {
    let rules = parse_rules(&[
        json!({ "output": "DP-1", "workspace": "2", "wallpaper": "static:a.jpg" }),
        json!({ "output": "DP-1", "workspace": "3", "wallpaper": "video:clip.mp4" }),
        json!({ "output": "DP-2", "workspace": "1", "wallpaper": "static:other.jpg" }),
        json!({ "workspace": "4", "wallpaper": "static:any.jpg" }),
        json!({ "output": "DP-1", "workspace": "5", "wallpaper": "static:a.jpg" }),
    ]);
    let got = preload_paths(&rules, None, "DP-1", "/w", "/v");
    assert_eq!(got, vec!["/w/a.jpg".to_string(), "/w/any.jpg".to_string()]);
    let base = base_entry("/w/base.jpg");
    let with_base = preload_paths(&rules, Some(&base), "DP-1", "/w", "/v");
    assert_eq!(with_base[0], "/w/base.jpg");
    let video_base = BaseWallpaper {
        ty: "video".into(),
        path: "/v/x.mp4".into(),
        we_id: String::new(),
        mute: true,
        volume: 100,
    };
    assert!(
        !preload_paths(&rules, Some(&video_base), "DP-1", "/w", "/v").contains(&"/v/x.mp4".into())
    );
}

fn rt(topo: HashMap<u64, WorkspaceInfo>) -> WorkspaceRuntime {
    WorkspaceRuntime {
        topo,
        pending: HashMap::new(),
        last: HashMap::new(),
        base: HashMap::new(),
        dirs: HashMap::new(),
        deadline: None,
    }
}

fn base_entry(path: &str) -> BaseWallpaper {
    BaseWallpaper {
        ty: "static".into(),
        path: path.into(),
        we_id: String::new(),
        mute: true,
        volume: 100,
    }
}

#[test]
fn compute_updates_diff() {
    let rules = parse_rules(&[
        json!({ "output": "DP-1", "workspace": "2", "wallpaper": "a" }),
        json!({ "output": "DP-2", "workspace": "1", "wallpaper": "b" }),
    ]);
    let mut runtime =
        rt(topo(&[(1, "DP-1", 1, false), (2, "DP-1", 2, true), (3, "DP-2", 1, true)]));
    let mut got = compute_updates(&runtime, &rules);
    got.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        got,
        vec![
            ("DP-1".to_string(), DesiredWallpaper::Pin("a".to_string())),
            ("DP-2".to_string(), DesiredWallpaper::Pin("b".to_string()))
        ]
    );

    runtime.last.insert("DP-1".to_string(), "a".to_string());
    assert_eq!(
        compute_updates(&runtime, &rules),
        vec![("DP-2".to_string(), DesiredWallpaper::Pin("b".to_string()))]
    );
}

#[test]
fn refresh_pending_deadline() {
    let rules = parse_rules(&[json!({ "output": "DP-1", "workspace": "2", "wallpaper": "a" })]);
    let mut runtime = rt(topo(&[(2, "DP-1", 2, true)]));
    let deadline = Instant::now() + Duration::from_millis(120);
    assert!(refresh_pending(&mut runtime, &rules, deadline));
    assert_eq!(runtime.pending.get("DP-1"), Some(&DesiredWallpaper::Pin("a".to_string())));
    assert_eq!(runtime.deadline, Some(deadline));

    runtime.deadline = None;
    assert!(!refresh_pending(&mut runtime, &rules, deadline));
    assert_eq!(runtime.deadline, None);
}

#[test]
fn unpin_restores_base() {
    let rules = parse_rules(&[json!({ "output": "DP-1", "workspace": "2", "wallpaper": "a" })]);
    let mut runtime = rt(topo(&[(9, "DP-1", 9, true)]));
    assert!(compute_updates(&runtime, &rules).is_empty());

    runtime.last.insert("DP-1".to_string(), "a".to_string());
    assert!(compute_updates(&runtime, &rules).is_empty());

    runtime.base.insert("DP-1".to_string(), base_entry("/w/base.jpg"));
    assert_eq!(
        compute_updates(&runtime, &rules),
        vec![("DP-1".to_string(), DesiredWallpaper::Base)]
    );

    runtime.pending.insert("DP-1".to_string(), DesiredWallpaper::Base);
    assert!(compute_updates(&runtime, &rules).is_empty());

    runtime.pending.clear();
    runtime.last.remove("DP-1");
    assert!(compute_updates(&runtime, &rules).is_empty());
}

#[test]
fn base_json_roundtrip() {
    let mut base = HashMap::new();
    base.insert("DP-1".to_string(), base_entry("/w/a.jpg"));
    base.insert(
        "DP-2".to_string(),
        BaseWallpaper {
            ty: "video".into(),
            path: "/v/b.mp4".into(),
            we_id: String::new(),
            mute: false,
            volume: 40,
        },
    );
    let parsed = base_from_json(&base_to_json(&base));
    assert_eq!(parsed, base);
    let sparse = base_from_json(&json!({
        "DP-3": { "type": "", "path": "/x.png" },
        "DP-4": { "type": "static", "path": "", "we_id": "" },
        "DP-5": { "type": "we", "path": "", "we_id": "123" },
    }));
    assert_eq!(sparse.len(), 1);
    assert_eq!(sparse["DP-5"].we_id, "123");
}

#[test]
fn seed_last_pins() {
    let rules = parse_rules(&[
        json!({ "output": "DP-3", "workspace": "3", "wallpaper": "static:pin.webp" }),
        json!({ "output": "DP-3", "workspace": "5", "wallpaper": "we:42" }),
    ]);
    let outputs = json!({
        "DP-3": { "type": "static", "path": "/w/pin.webp", "we_id": "" },
        "DP-2": { "type": "static", "path": "/w/other.png", "we_id": "" },
        "*": { "type": "static", "path": "/w/pin.webp" },
    });
    let last = seed_last(&outputs, &rules, "/w", "/v");
    assert_eq!(last.get("DP-3").map(String::as_str), Some("static:pin.webp"));
    assert!(!last.contains_key("DP-2"));
    assert!(!last.contains_key("*"));

    let we_outputs = json!({ "DP-3": { "type": "we", "path": "", "we_id": "42" } });
    assert_eq!(
        seed_last(&we_outputs, &rules, "/w", "/v").get("DP-3").map(String::as_str),
        Some("we:42")
    );
}

#[test]
fn hypr_topology() {
    let monitors = r#"[
        {"name": "DP-1", "activeWorkspace": {"id": 2, "name": "2"}},
        {"name": "DP-2", "activeWorkspace": {"id": 5, "name": "mail"}}
    ]"#;
    let workspaces = r#"[
        {"id": 1, "name": "1", "monitor": "DP-1"},
        {"id": 2, "name": "2", "monitor": "DP-1"},
        {"id": 5, "name": "mail", "monitor": "DP-2"},
        {"id": -99, "name": "special:scratch", "monitor": "DP-1"}
    ]"#;
    let topo = hypr::build_topology(monitors, workspaces).expect("parses");
    assert_eq!(topo.len(), 3);
    assert!(topo[&2].active && topo[&2].output == "DP-1");
    assert!(!topo[&1].active);
    assert_eq!(topo[&5].name.as_deref(), Some("mail"));
    assert_eq!(topo[&2].name, None);
    assert_eq!(topo[&5].idx, 5);
}

#[test]
fn hypr_event_filter() {
    for line in [
        "workspace>>3",
        "workspacev2>>3,web",
        "focusedmonv2>>DP-2,5",
        "moveworkspacev2>>2,2,DP-2",
        "createworkspacev2>>7,scratch",
        "destroyworkspacev2>>7,scratch",
        "renameworkspace>>2,newname",
        "monitoradded>>DP-3",
        "monitorremoved>>DP-3",
    ] {
        assert!(hypr::relevant_event(line), "{line}");
    }
    for line in ["activewindow>>kitty,~", "openlayer>>bar", "screencast>>1,0"] {
        assert!(!hypr::relevant_event(line), "{line}");
    }
}

#[test]
fn backend_priority() {
    assert_eq!(classify_backend(true, false, "niri"), Some(Backend::Niri));
    assert_eq!(classify_backend(true, true, "Hyprland"), Some(Backend::Niri));
    assert_eq!(classify_backend(false, true, "Hyprland"), Some(Backend::Hyprland));
    assert_eq!(classify_backend(false, true, ""), Some(Backend::Hyprland));
    assert_eq!(classify_backend(false, false, "KDE"), Some(Backend::Kwin));
    assert_eq!(classify_backend(false, false, "plasmawayland"), Some(Backend::Kwin));
    assert_eq!(classify_backend(false, false, "GNOME"), None);
    assert_eq!(classify_backend(false, false, ""), None);
}

#[test]
fn hypr_topology_edge_cases() {
    let disabled = r#"[
        {"name": "DP-1", "activeWorkspace": {"id": 2, "name": "2"}},
        {"name": "DP-2"}
    ]"#;
    let wss = r#"[
        {"id": 2, "name": "2", "monitor": "DP-1"},
        {"id": 9, "name": "9", "monitor": "DP-2"}
    ]"#;
    let topo = hypr::build_topology(disabled, wss).expect("parses");
    assert_eq!(topo.len(), 2);
    assert!(topo[&2].active);
    assert!(!topo[&9].active);

    assert!(hypr::build_topology("[]", "[]").is_some_and(|topo| topo.is_empty()));
    assert!(hypr::build_topology("not json", "[]").is_none());
    assert!(hypr::build_topology("{}", "[]").is_none());
    assert!(hypr::build_topology("[]", "42").is_none());
}

#[test]
fn kwin_dbus_parse() {
    let desktops = r#"method return time=1710.1 sender=:1.23 -> destination=:1.99 serial=5 reply_serial=2
   variant       array [
         struct {
            uint32 0
            string "a1b2-uuid-1"
            string "Desktop 1"
         }
         struct {
            uint32 1
            string "c3d4-uuid-2"
            string "Work"
         }
      ]
"#;
    let parsed = kwin::parse_desktops_reply(desktops);
    assert_eq!(
        parsed,
        vec![
            (0, String::from("a1b2-uuid-1"), String::from("Desktop 1")),
            (1, String::from("c3d4-uuid-2"), String::from("Work")),
        ]
    );
    let current = "method return time=1710.1 sender=:1.23 -> destination=:1.99 serial=6 reply_serial=3\n   variant       string \"c3d4-uuid-2\"\n";
    assert_eq!(kwin::parse_current_reply(current).as_deref(), Some("c3d4-uuid-2"));
    assert!(kwin::is_desktop_signal(
        "signal time=1710.2 sender=:1.4 -> destination=(null destination) serial=8 path=/VirtualDesktopManager; interface=org.kde.KWin.VirtualDesktopManager; member=currentChanged"
    ));
    assert!(!kwin::is_desktop_signal("signal ... member=showingDesktopChanged"));
}

#[test]
fn kwin_topology() {
    let desktops = vec![
        (0, String::from("u1"), String::from("Desktop 1")),
        (1, String::from("u2"), String::from("Work")),
    ];
    let outs = vec![String::from("DP-1"), String::from("DP-2")];
    let topo = kwin::build_topology(&desktops, "u2", &outs);
    assert_eq!(topo.len(), 4);
    let active: Vec<&WorkspaceInfo> = topo.values().filter(|ws| ws.active).collect();
    assert_eq!(active.len(), 2);
    assert!(active.iter().all(|ws| ws.idx == 2));
    assert!(topo.values().any(|ws| ws.output == "DP-1" && ws.name.as_deref() == Some("Work")));
}
