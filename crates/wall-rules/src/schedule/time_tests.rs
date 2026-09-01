use super::*;

#[test]
fn time_parsing() {
    assert_eq!(parse_at("sunrise"), Some(At::Sunrise(0)));
    assert_eq!(parse_at("SUNSET"), Some(At::Sunset(0)));
    assert_eq!(parse_at("sunrise+30"), Some(At::Sunrise(30)));
    assert_eq!(parse_at("sunset-90"), Some(At::Sunset(-90)));
    assert_eq!(parse_at("22:30"), Some(At::Clock(22 * 60 + 30)));
    assert_eq!(parse_at("06:05"), Some(At::Clock(365)));
    assert_eq!(parse_at("00:00"), Some(At::Clock(0)));
    assert_eq!(parse_at("23:59"), Some(At::Clock(23 * 60 + 59)));
    assert_eq!(parse_at("24:00"), None);
    assert_eq!(parse_at("12:60"), None);
    assert_eq!(parse_at("nonsense"), None);
    assert_eq!(parse_at("sunrise+721"), None);
}

#[test]
fn solar_offsets_wrap() {
    assert_eq!(fire_minute(At::Sunrise(-90), 60, 1200), 1410);
    assert_eq!(fire_minute(At::Sunset(300), 360, 1260), 120);
}
