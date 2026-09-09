use super::*;
use serde_json::json;

fn monitor() -> Monitor {
    let mut monitor = Monitor::default();
    monitor
        .set_outputs(&json!({"Ok":{"Outputs":{
            "DP-1":{"logical":{"width":1000,"height":800}},
            "DP-2":{"logical":{"width":1600,"height":900}}
        }}}))
        .unwrap();
    monitor.event(&json!({"WorkspacesChanged":{"workspaces":[
        {"id":1,"output":"DP-1","is_active":true},
        {"id":2,"output":"DP-1","is_active":false},
        {"id":3,"output":"DP-2","is_active":true}
    ]}}));
    monitor.event(&json!({"WindowsChanged":{"windows":[{
        "id":8,"workspace_id":1,"is_floating":false,
        "layout":{"pos_in_scrolling_layout":[1,1],"tile_size":[968,768],"tile_pos_in_workspace_view":[16,16]}
    }]}}));
    monitor
}

#[test]
fn wide_columns_follow_workspace_visibility_and_output_moves() {
    let mut monitor = monitor();
    assert!(monitor.snapshot().supported);
    assert_eq!(monitor.snapshot().outputs, HashSet::from(["DP-1".into()]));
    monitor.event(&json!({"WorkspaceActivated":{"id":2,"focused":true}}));
    assert!(monitor.snapshot().outputs.is_empty());
    monitor.event(&json!({"WorkspaceActivated":{"id":1,"focused":false}}));
    assert_eq!(monitor.snapshot().outputs.len(), 1);
    monitor.event(&json!({"WindowOpenedOrChanged":{"window":{
        "id":8,"workspace_id":3,"is_floating":false,
        "layout":{"pos_in_scrolling_layout":[1,1],"tile_size":[1568,868],"tile_pos_in_workspace_view":[16,16]}
    }}}));
    assert_eq!(monitor.snapshot().outputs, HashSet::from(["DP-2".into()]));
    monitor.event(&json!({"WindowClosed":{"id":8}}));
    assert!(monitor.snapshot().outputs.is_empty());
}

#[test]
fn column_pause_requires_ninety_percent_visible_width_and_tiling() {
    let mut monitor = monitor();
    for (width, x, expected) in
        [(900, 0, true), (899, 0, false), (968, -100, false), (968, 1000, false), (968, 16, true)]
    {
        monitor.event(&json!({"WindowLayoutsChanged":{"changes":[[8,{
            "pos_in_scrolling_layout":[1,1],"tile_size":[width,768],"tile_pos_in_workspace_view":[x,16]
        }]]}}));
        assert_eq!(!monitor.snapshot().outputs.is_empty(), expected);
    }
    monitor.windows.get_mut(&8).unwrap().is_floating = true;
    assert!(monitor.snapshot().outputs.is_empty());
    monitor.windows.get_mut(&8).unwrap().is_floating = false;
    monitor.windows.get_mut(&8).unwrap().layout.tile_pos_in_workspace_view = None;
    assert!(monitor.snapshot().outputs.is_empty());
    monitor.event(&json!({"WindowsChanged":{"windows":[]}}));
    assert!(monitor.snapshot().outputs.is_empty());
}

#[test]
fn missing_geometry_and_removed_outputs_do_not_pause() {
    let mut monitor = monitor();
    assert_eq!(monitor.event(&json!({"WindowLayoutsChanged":{"changes":"invalid"}})), None);
    monitor.set_outputs(&json!({"Ok":{"Outputs":{}}})).unwrap();
    assert!(!monitor.snapshot().supported);
    assert!(monitor.snapshot().outputs.is_empty());
    assert!(!Monitor::default().snapshot().supported);
}

#[test]
fn niri_tiled_geometry_without_positions_uses_the_active_column() {
    let mut monitor = monitor();
    monitor.windows.get_mut(&8).unwrap().layout.tile_pos_in_workspace_view = None;
    monitor.event(&json!({"WorkspaceActiveWindowChanged":{"workspace_id":1,"active_window_id":8}}));
    assert_eq!(monitor.snapshot().outputs, HashSet::from(["DP-1".into()]));
    monitor.event(&json!({"WindowOpenedOrChanged":{"window":{
        "id":9,"workspace_id":1,"is_floating":false,
        "layout":{"pos_in_scrolling_layout":[2,1],"tile_size":[468,768],"tile_pos_in_workspace_view":null}
    }}}));
    monitor.event(&json!({"WorkspaceActiveWindowChanged":{"workspace_id":1,"active_window_id":9}}));
    assert!(monitor.snapshot().outputs.is_empty());
    monitor.event(&json!({"WorkspaceActiveWindowChanged":{"workspace_id":1,"active_window_id":8}}));
    assert_eq!(monitor.snapshot().outputs.len(), 1);
    monitor.event(&json!({"WorkspaceActivated":{"id":2,"focused":true}}));
    assert!(monitor.snapshot().outputs.is_empty());
}
