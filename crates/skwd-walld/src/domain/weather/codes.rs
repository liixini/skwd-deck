#[allow(clippy::match_same_arms)]
pub(crate) fn map_weather(code: i64, wind_kmh: f64) -> Vec<String> {
    let mut weather = match code {
        0 => vec!["clear", "sunny"],
        1 => vec!["clear"],
        2 | 3 => vec!["cloudy"],
        45 | 48 => vec!["foggy"],
        51..=67 | 80..=82 => vec!["rainy"],
        71..=77 | 85 | 86 => vec!["snowy"],
        95..=99 => vec!["stormy"],
        _ => vec!["clear"],
    };
    if wind_kmh >= 30.0 {
        weather.push("windy");
    }
    weather.into_iter().map(String::from).collect()
}
