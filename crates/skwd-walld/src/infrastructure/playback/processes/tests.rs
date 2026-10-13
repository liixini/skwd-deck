use super::*;
#[test]
fn matches_exact_names() {
    let running = vec!["Overwatch.exe".into(), "steamwebhelper".into(), "/usr/bin/mpv".into()];
    assert_eq!(matches("overwatch,mpv", &running), vec!["mpv", "overwatch"]);
    assert!(matches("watch,steam,wine", &running).is_empty());
    assert_eq!(normalized("Z:\\Games\\Overwatch.EXE"), "overwatch.exe");
    assert!(matches("", &running).is_empty());
}

#[test]
fn matches_wildcard_names() {
    for (pattern, name, expected) in [
        ("steam*", "steam", true),
        ("steam*", "steamwebhelper", true),
        ("steam*", "xsteam", false),
        ("*watch", "overwatch", true),
        ("*watch", "watchdog", false),
        ("a*b*c", "abbbc", true),
        ("a*b*c", "abbbd", false),
        ("**", "", true),
        ("game?", "game2", true),
        ("game?", "gameé", true),
        ("game?", "game", false),
        ("game?", "game22", false),
        ("game?.*", "game2.bin", true),
        ("game[1].bin", "game1xbin", false),
        ("game[1].bin", "game[1].bin", true),
        ("game+", "game+", true),
        ("game+", "gameee", false),
    ] {
        assert_eq!(pattern_matches(pattern, name), expected, "{pattern}: {name}");
    }
}

#[test]
fn normalizes_wildcard_rules() {
    let running = vec!["Z:\\Games\\Overwatch.EXE".into(), "/usr/bin/steamwebhelper".into()];
    assert_eq!(matches(" Over*.exe,steam*\nOver*.exe,, ", &running), vec!["Over*.exe", "steam*"]);
    assert_eq!(matches("*.exe", &running), vec!["*.exe"]);
    assert!(matches("*.exe,steam*.exe", &["steamwebhelper".into()]).is_empty());
    assert_eq!(matches("Overwatch.exe", &["overwatch".into()]), vec!["Overwatch.exe"]);
    assert_eq!(matches("Overwatc?", &running), vec!["Overwatc?"]);
    assert_eq!(matches("Overwatch.exe*,*exe", &running), vec!["*exe", "Overwatch.exe*"]);
    assert!(matches("watch*,steam?", &running).is_empty());
}
