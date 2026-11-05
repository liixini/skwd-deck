use super::*;

#[test]
fn mode_keys_round_trip() {
    for mode in [StudioMode::Dark, StudioMode::Light, StudioMode::ByTime, StudioMode::FollowTheme] {
        assert_eq!(StudioMode::from_key(mode.key()), mode);
    }
    assert_eq!(StudioMode::from_key("time"), StudioMode::FollowTheme);
}

#[test]
fn clock_switches_at_boundaries() {
    for (minute, dark) in [(419, true), (420, false), (1139, false), (1140, true)] {
        assert_eq!(dark_at(minute), dark);
    }
}
