use super::*;
#[test]
fn matches_executables_without_matching_arguments_or_similar_names() {
    let running = vec!["Overwatch.exe".into(), "steamwebhelper".into(), "/usr/bin/mpv".into()];
    assert_eq!(matches("overwatch,mpv", &running), vec!["mpv", "overwatch"]);
    assert!(matches("watch,steam,wine", &running).is_empty());
    assert_eq!(normalized("Z:\\Games\\Overwatch.EXE"), "overwatch");
    assert!(matches("", &running).is_empty());
}
