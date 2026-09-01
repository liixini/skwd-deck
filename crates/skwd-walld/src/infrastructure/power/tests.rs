use super::{PolicyTracker, PowerSourceState, is_power_event};

fn assert_settled(tracker: &mut PolicyTracker, source: PowerSourceState) {
    let observation = tracker.observe(source).unwrap();
    assert!(!observation.source_changed);
    assert!(!observation.reconcile_needed);
    assert!(!observation.retrying);
}

#[test]
fn unknown_source_ignored() {
    let mut tracker = PolicyTracker::new(PowerSourceState::OnBattery);
    assert!(tracker.observe(PowerSourceState::Unknown).is_none());
    assert_settled(&mut tracker, PowerSourceState::OnBattery);
}

#[test]
fn discovers_battery_late() {
    let mut tracker = PolicyTracker::new(PowerSourceState::NoSystemBattery);
    assert_settled(&mut tracker, PowerSourceState::NoSystemBattery);

    let observation = tracker.observe(PowerSourceState::OnBattery).unwrap();
    assert!(observation.source_changed);
    assert!(observation.reconcile_needed);
    assert!(!observation.retrying);
}

#[test]
fn failed_refresh_pending() {
    let mut tracker = PolicyTracker::new(PowerSourceState::ExternalPower);
    let first = tracker.observe(PowerSourceState::OnBattery).unwrap();
    assert!(first.source_changed);
    assert!(first.reconcile_needed);
    assert!(!first.retrying);

    tracker.mark_failed();
    let retry = tracker.observe(PowerSourceState::OnBattery).unwrap();
    assert!(!retry.source_changed);
    assert!(retry.reconcile_needed);
    assert!(retry.retrying);
    assert_eq!(tracker.applied, Some(false));

    tracker.mark_current(true);
    assert_settled(&mut tracker, PowerSourceState::OnBattery);
}

#[test]
fn pending_failure_reconciles() {
    let mut tracker = PolicyTracker::new(PowerSourceState::ExternalPower);
    tracker.observe(PowerSourceState::OnBattery).unwrap();
    tracker.mark_failed();

    let recovery = tracker.observe(PowerSourceState::ExternalPower).unwrap();
    assert!(recovery.source_changed);
    assert!(recovery.reconcile_needed);
    assert!(recovery.retrying);
}

#[test]
fn power_supply_uevents() {
    assert!(is_power_event(b"change@/devices/pci/BAT0\0ACTION=change\0SUBSYSTEM=power_supply\0"));
    assert!(!is_power_event(b"change@/devices/pci/card0\0ACTION=change\0SUBSYSTEM=drm\0"));
}

#[tokio::test]
async fn request_refresh_wakes() {
    super::request_refresh();
    let wake = super::next_wake(None, false).await.unwrap();
    assert_eq!(wake, super::PowerWake::Refresh);
}
