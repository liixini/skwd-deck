use skwd_wall_core::lock;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::domain::weather::map_weather;

const TTL: Duration = Duration::from_secs(3600);

static CACHE: Mutex<Option<(Instant, Vec<String>)>> = Mutex::new(None);

fn parse_geocode(json: &str) -> Option<(f64, f64)> {
    let val: Value = serde_json::from_str(json).ok()?;
    let first = val.get("results")?.as_array()?.first()?;
    Some((first.get("latitude")?.as_f64()?, first.get("longitude")?.as_f64()?))
}

fn parse_forecast(json: &str) -> Option<(i64, f64)> {
    let cur = serde_json::from_str::<Value>(json).ok()?.get("current")?.clone();
    let code = cur.get("weather_code")?.as_i64()?;
    let wind = cur.get("wind_speed_10m").and_then(Value::as_f64).unwrap_or(0.0);
    Some((code, wind))
}

fn geocode(locale: &str) -> Option<(f64, f64)> {
    let body = crate::infrastructure::http::agent()
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .set("User-Agent", crate::infrastructure::http::USER_AGENT)
        .query("name", locale)
        .query("count", "1")
        .call()
        .ok()?
        .into_string()
        .ok()?;
    parse_geocode(&body)
}

fn fetch_forecast(lat: f64, lon: f64) -> Option<(i64, f64)> {
    let body = crate::infrastructure::http::agent()
        .get("https://api.open-meteo.com/v1/forecast")
        .set("User-Agent", crate::infrastructure::http::USER_AGENT)
        .query("latitude", &format!("{lat:.4}"))
        .query("longitude", &format!("{lon:.4}"))
        .query("current", "weather_code,wind_speed_10m")
        .call()
        .ok()?
        .into_string()
        .ok()?;
    parse_forecast(&body)
}

pub fn current(locale: &str, lat: f64, lon: f64) -> Vec<String> {
    {
        let guard = lock(&CACHE);
        if let Some((at, tags)) = guard.as_ref()
            && at.elapsed() < TTL
        {
            return tags.clone();
        }
    }
    let coords = if lat != 0.0 || lon != 0.0 { Some((lat, lon)) } else { geocode(locale) };
    let Some((latitude, longitude)) = coords else { return Vec::new() };
    let Some((code, wind)) = fetch_forecast(latitude, longitude) else { return Vec::new() };
    let tags = map_weather(code, wind);
    *lock(&CACHE) = Some((Instant::now(), tags.clone()));
    tags
}

#[cfg(test)]
mod tests;
