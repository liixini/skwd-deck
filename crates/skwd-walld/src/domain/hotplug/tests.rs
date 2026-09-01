use super::*;

fn outputs(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

#[test]
fn per_output_divergence() {
    assert!(!per_output_state_is_divergent(&[]));
    assert!(!per_output_state_is_divergent(&outputs(&["*"])));
    assert!(per_output_state_is_divergent(&outputs(&["DP-1"])));
    assert!(per_output_state_is_divergent(&outputs(&["DP-1", "HDMI-A-1"])));
}

#[test]
fn new_outputs_live_order() {
    let persisted = outputs(&["DP-1"]);
    assert_eq!(
        newly_appeared_outputs(&persisted, &outputs(&["DP-1", "HDMI-A-1"])),
        outputs(&["HDMI-A-1"])
    );
    assert!(newly_appeared_outputs(&persisted, &outputs(&["DP-1"])).is_empty());
}

#[test]
fn retained_persisted_order() {
    let persisted = outputs(&["DP-1", "DP-2", "DP-3"]);
    assert_eq!(
        retained_outputs(&persisted, &outputs(&["DP-1", "DP-2"])),
        outputs(&["DP-1", "DP-2"])
    );
    assert_eq!(retained_outputs(&outputs(&["*"]), &outputs(&["DP-1"])), outputs(&["*"]));
    assert_eq!(retained_outputs(&persisted, &outputs(&["DP-1", "DP-2", "DP-3"])), persisted);
}

#[test]
fn uniform_renderer_plan() {
    let uniform = outputs(&["*"]);
    assert_eq!(
        hotplug_plan(&uniform, Some(AppliedKind::Video), true, true, false),
        HotplugPlan::RespawnUniformVideo
    );
    assert_eq!(
        hotplug_plan(&uniform, Some(AppliedKind::Video), false, true, false),
        HotplugPlan::KeepAlive
    );
    assert_eq!(
        hotplug_plan(&uniform, Some(AppliedKind::Video), false, false, false),
        HotplugPlan::RespawnUniformVideo
    );
    assert_eq!(
        hotplug_plan(&uniform, Some(AppliedKind::Static), false, false, true),
        HotplugPlan::KeepAlive
    );
    assert_eq!(
        hotplug_plan(&uniform, Some(AppliedKind::Static), false, false, false),
        HotplugPlan::RespawnUniformStatic
    );
    assert_eq!(
        hotplug_plan(&uniform, Some(AppliedKind::Other), false, false, false),
        HotplugPlan::Reconcile
    );
    assert_eq!(
        hotplug_plan(&outputs(&["DP-1"]), Some(AppliedKind::Video), false, false, false),
        HotplugPlan::Reconcile
    );
    assert_eq!(
        hotplug_plan(&outputs(&["*", "DP-2"]), Some(AppliedKind::Static), false, false, false),
        HotplugPlan::Reconcile
    );
}

#[test]
fn representative_state_key() {
    let live = outputs(&["DP-1", "HDMI-A-1"]);

    assert_eq!(
        representative(&outputs(&["HDMI-A-1"]), &live),
        Some(Representative {
            output: String::from("HDMI-A-1"),
            state_key: String::from("HDMI-A-1"),
        })
    );
    assert_eq!(
        representative(&outputs(&["*"]), &live),
        Some(Representative { output: String::from("DP-1"), state_key: String::from("*") })
    );
    assert_eq!(
        representative(&outputs(&["OLD-1"]), &live),
        Some(Representative { output: String::from("OLD-1"), state_key: String::from("OLD-1") })
    );
    assert_eq!(representative(&[], &live), None);
}
