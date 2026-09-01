use super::map_weather;

const WEATHER_VALUES: [&str; 8] =
    ["clear", "sunny", "cloudy", "rainy", "snowy", "stormy", "foggy", "windy"];

#[test]
fn tags_in_vocabulary() {
    let vocabulary = WEATHER_VALUES;
    for code in [0, 1, 2, 3, 45, 48, 55, 63, 71, 81, 86, 95, 999] {
        for wind in [0.0, 45.0] {
            for tag in map_weather(code, wind) {
                assert!(vocabulary.contains(&tag.as_str()), "code {code} tag '{tag}'");
            }
        }
    }
}

#[test]
fn maps_wmo_codes() {
    assert_eq!(map_weather(0, 0.0), ["clear", "sunny"]);
    assert_eq!(map_weather(3, 0.0), ["cloudy"]);
    assert_eq!(map_weather(48, 0.0), ["foggy"]);
    assert_eq!(map_weather(63, 0.0), ["rainy"]);
    assert_eq!(map_weather(75, 0.0), ["snowy"]);
    assert_eq!(map_weather(95, 0.0), ["stormy"]);
    assert_eq!(map_weather(999, 0.0), ["clear"]);
}

#[test]
fn strong_wind_adds_windy() {
    assert_eq!(map_weather(0, 40.0), ["clear", "sunny", "windy"]);
    assert_eq!(map_weather(63, 35.0), ["rainy", "windy"]);
    assert!(!map_weather(0, 29.9).contains(&String::from("windy")));
}
