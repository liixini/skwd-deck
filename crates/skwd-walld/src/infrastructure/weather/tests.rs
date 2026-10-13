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

#[test]
fn cache_tracks_location() {
    let location = Location {
        locale: "London".into(),
        lat: 0.0,
        lon: 0.0,
        endpoint: FORECAST_ENDPOINT.into(),
        geocode_endpoint: GEOCODE_ENDPOINT.into(),
    };
    let now = Instant::now();
    let cache = Cached {
        location: location.clone(),
        checked: now,
        forecast: Forecast {
            weather: vec!["clear".into()],
            fetched_at: Some(42),
            ..Forecast::default()
        },
    };
    assert!(cache.matches(&location, now));
    assert!(!cache.matches(&Location { locale: "Stockholm".into(), ..location.clone() }, now));
    assert!(!cache.matches(&Location { lat: 59.3, lon: 18.0, ..location.clone() }, now));
    assert!(!cache.matches(
        &Location { endpoint: "http://localhost/weather".into(), ..location.clone() },
        now
    ));
    assert!(!cache.matches(&location, now + refresh_interval()));
    assert_eq!(cache.forecast.fetched_at, Some(42));
}

#[test]
fn cache_read_never_fetches() {
    let (_guard, _root) = crate::testenv::lock();
    let fetch = lock(&FETCH_LOCK);
    let (sender, receiver) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        sender.send(cached("offline", 52.0, 18.0)).unwrap();
    });
    let result = receiver.recv_timeout(Duration::from_secs(1));
    drop(fetch);
    reader.join().unwrap();
    assert!(result.unwrap().is_none());
}

#[test]
fn passive_cache_is_keyed() {
    let (_guard, _root) = crate::testenv::lock();
    let location = location("offline", 52.0, 18.0).unwrap();
    let previous = lock(&CACHE).replace(Cached {
        location,
        checked: Instant::now().checked_sub(refresh_interval()).unwrap(),
        forecast: Forecast {
            weather: vec!["rainy".into()],
            fetched_at: Some(42),
            ..Forecast::default()
        },
    });
    assert_eq!(cached("offline", 52.0, 18.0).unwrap().fetched_at, Some(42));
    assert_eq!(cached("offline", 52.0, 18.0).unwrap().weather, ["rainy"]);
    assert!(cached("elsewhere", 53.0, 18.0).is_none());
    *lock(&CACHE) = previous;
}

#[test]
fn city_precedes_coordinates() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for body in [
            r#"{"results":[{"latitude":51.5074,"longitude":-0.1278}]}"#,
            r#"{"current":{"weather_code":61}}"#,
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 4096];
            let count = stream.read(&mut buffer).unwrap();
            requests.push(String::from_utf8_lossy(&buffer[..count]).to_string());
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        requests
    });
    let location = Location {
        locale: "London".into(),
        lat: 59.3,
        lon: 18.0,
        endpoint: endpoint.clone(),
        geocode_endpoint: endpoint,
    };
    let forecast = fetch_forecast(&location).unwrap();
    assert_eq!(forecast.weather, ["rainy"]);
    assert!(forecast.fetched_at.is_some());
    let requests = server.join().unwrap();
    assert!(requests[0].contains("name=London"));
    assert!(requests[1].contains("latitude=51.5074"));
    assert!(requests[1].contains("longitude=-0.1278"));
    assert!(!requests[1].contains("59.3000"));
}

#[test]
fn rich_forecast_stays_optional() {
    let response: Response = serde_json::from_str(r#"{"current":{"time":1781524800,"weather_code":3,"temperature_2m":19.5,"cloud_cover":100,"snow_depth":0,"is_day":1},"utc_offset_seconds":7200}"#).unwrap();
    assert_eq!(response.current.time.unwrap().as_i64(), Some(1_781_524_800));
    assert_eq!(response.current.temperature_2m, Some(19.5));
    assert_eq!(response.current.snow_depth, Some(0.0));
    assert_eq!(response.utc_offset_seconds, 7200);
    let legacy: Response = serde_json::from_str(r#"{"current":{"weather_code":3}}"#).unwrap();
    assert!(legacy.current.snow_depth.is_none());
    assert!(legacy.current.is_day.is_none());
}

#[test]
fn saved_forecast_location_bound() {
    let saved = Saved {
        location: Location {
            locale: "London".into(),
            lat: 0.0,
            lon: 0.0,
            endpoint: FORECAST_ENDPOINT.into(),
            geocode_endpoint: GEOCODE_ENDPOINT.into(),
        },
        forecast: Forecast {
            weather: vec!["rainy".into()],
            fetched_at: Some(42),
            ..Forecast::default()
        },
    };
    let restored: Saved = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert!(restored.location == saved.location);
    assert!(restored.location != Location { locale: "Tokyo".into(), ..saved.location });
    assert_eq!(restored.forecast, saved.forecast);
}

#[test]
fn failed_fetch_restores_stale() {
    use std::io::{Read, Write};
    let (_guard, _root) = crate::testenv::lock();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        for body in [r#"{"current":{"weather_code":3,"time":1781524800}}"#, "{}", "{}"] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 4096];
            assert!(stream.read(&mut buffer).unwrap() > 0);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let previous_endpoint = std::env::var_os(ENDPOINT_ENV);
    unsafe {
        std::env::set_var(ENDPOINT_ENV, endpoint);
    }
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":59.3,"longitude":18.0}}),
    );
    let state = std::sync::Arc::new(skwd_wall_core::WallState::open().unwrap());
    let first = outdoors_forecast(&state.config(), true);
    assert_eq!(first.weather, ["cloudy"]);
    let previous_cache = lock(&CACHE).take();
    let restored = outdoors_forecast(&state.config(), true);
    assert!(restored.stale);
    assert_eq!(restored.fetched_at, first.fetched_at);
    assert_eq!(restored.observation, first.observation);
    crate::testenv::write_config(
        serde_json::json!({"schedule":{"latitude":35.0,"longitude":139.0}}),
    );
    state.reload_config();
    assert!(outdoors_forecast(&state.config(), true).weather.is_empty());
    *lock(&CACHE) = previous_cache;
    unsafe {
        if let Some(endpoint) = previous_endpoint {
            std::env::set_var(ENDPOINT_ENV, endpoint);
        } else {
            std::env::remove_var(ENDPOINT_ENV);
        }
    }
    server.join().unwrap();
}
