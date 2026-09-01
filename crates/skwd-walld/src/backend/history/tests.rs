use super::ApplySource;

#[test]
fn source_policy() {
    use ApplySource::*;
    let table = [
        (User, true, false, true, true),
        (UserOverride, true, false, false, true),
        (Random, true, true, true, true),
        (Rotation, false, true, true, true),
        (Playlist, false, false, true, true),
        (Schedule, false, false, true, true),
        (Workspace, false, false, true, true),
        (Restore, false, false, false, false),
        (Hotplug, false, false, false, false),
        (Replay, false, false, true, true),
    ];

    for (source, records, random, locks, updates_restore_policy) in table {
        assert_eq!(source.records(), records, "{source:?}");
        assert_eq!(source.broadcast_random(), random, "{source:?}");
        assert_eq!(source.respects_output_locks(), locks, "{source:?}");
        assert_eq!(source.updates_restore_policy(), updates_restore_policy, "{source:?}");
    }
}
