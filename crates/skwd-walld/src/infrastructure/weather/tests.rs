#![cfg(test)]

use super::*;

#[test]
fn parse_forecast_current() {
    let json = r#"{"current":{"time":"2026-07-16T12:00","weather_code":61,"wind_speed_10m":12.3}}"#;
    assert_eq!(parse_forecast(json), Some((61, 12.3)));
    assert_eq!(parse_forecast(r#"{"current":{"weather_code":0}}"#), Some((0, 0.0)));
    assert_eq!(parse_forecast(r#"{"error":true}"#), None);
    assert_eq!(parse_forecast("not json"), None);
}

#[test]
fn geocode_first_result() {
    let json = r#"{"results":[{"name":"London","latitude":51.5074,"longitude":-0.1278},{"name":"London, ON","latitude":42.98,"longitude":-81.24}]}"#;
    assert_eq!(parse_geocode(json), Some((51.5074, -0.1278)));
    assert_eq!(parse_geocode(r#"{"generationtime_ms":0.1}"#), None);
    assert_eq!(parse_geocode("{}"), None);
}
