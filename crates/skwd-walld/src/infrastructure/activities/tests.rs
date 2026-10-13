use super::*;

#[test]
fn activity_signals_refresh() {
    for member in [
        "CurrentActivityChanged",
        "ActivityAdded",
        "ActivityRemoved",
        "ActivityNameChanged",
        "ActivityStateChanged",
    ] {
        assert!(changed(&format!(
            "signal interface=org.kde.ActivityManager.Activities; member={member}"
        )));
    }
    assert!(!changed("method return sender=:1.2"));
    assert!(!changed("signal interface=org.freedesktop.DBus; member=NameAcquired"));
}
