use skwd_wall_core::lock;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use wall_rules::weather::map_weather;

const REFRESH: Duration = Duration::from_secs(15 * 60);
const FORECAST_ENDPOINT: &str = "https://api.open-meteo.com/v1/forecast";
const GEOCODE_ENDPOINT_ENV: &str = "SKWD_WEATHER_GEOCODE_ENDPOINT";
const GEOCODE_ENDPOINT: &str = "https://geocoding-api.open-meteo.com/v1/search";
const REFRESH_ENV: &str = "SKWD_WEATHER_REFRESH_SECONDS";
const ENDPOINT_ENV: &str = "SKWD_WEATHER_ENDPOINT";
const USER_AGENT_HEADER: &str = "User-Agent";
const NAME_QUERY: &str = "name";
const COUNT_QUERY: &str = "count";
const SINGLE_RESULT: &str = "1";
const LATITUDE_QUERY: &str = "latitude";
const LONGITUDE_QUERY: &str = "longitude";
const CURRENT_QUERY: &str = "current";
const CURRENT_FIELDS: &str =
    "weather_code,wind_speed_10m,temperature_2m,cloud_cover,snow_depth,is_day";
const TIME_FORMAT_QUERY: &str = "timeformat";
const TIME_FORMAT_UNIX: &str = "unixtime";
const TIMEZONE_QUERY: &str = "timezone";
const TIMEZONE_AUTO: &str = "auto";
const FORECAST_CACHE: &str = "outdoors-forecast.json";

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub(crate) struct Forecast {
    pub weather: Vec<String>,
    pub fetched_at: Option<u64>,
    pub observation: Option<Observation>,
    pub stale: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub(crate) struct Observation {
    pub latitude: f64,
    pub longitude: f64,
    pub utc_offset_seconds: i64,
    pub observed_at: Option<i64>,
    pub temperature: Option<f64>,
    pub cloud_cover: Option<f64>,
    pub snow_depth: Option<f64>,
    pub is_day: Option<bool>,
}

#[derive(Deserialize, Serialize)]
struct Saved {
    location: Location,
    forecast: Forecast,
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
struct Location {
    locale: String,
    lat: f64,
    lon: f64,
    endpoint: String,
    geocode_endpoint: String,
}

struct Cached {
    location: Location,
    checked: Instant,
    forecast: Forecast,
}

impl Cached {
    fn matches(&self, location: &Location, now: Instant) -> bool {
        self.location == *location && now.duration_since(self.checked) < refresh_interval()
    }
}

static FETCH_LOCK: Mutex<()> = Mutex::new(());
static CACHE: Mutex<Option<Cached>> = Mutex::new(None);

#[derive(Deserialize)]
struct Coordinates {
    latitude: f64,
    longitude: f64,
}

#[derive(Deserialize)]
struct Geocode {
    results: Vec<Coordinates>,
}

#[derive(Deserialize)]
struct Current {
    time: Option<serde_json::Value>,
    weather_code: i64,
    #[serde(default)]
    wind_speed_10m: f64,
    temperature_2m: Option<f64>,
    cloud_cover: Option<f64>,
    snow_depth: Option<f64>,
    is_day: Option<u8>,
}

#[derive(Deserialize)]
struct Response {
    current: Current,
    #[serde(default)]
    utc_offset_seconds: i64,
}

fn parse_geocode(json: &str) -> Option<(f64, f64)> {
    let response: Geocode = serde_json::from_str(json).ok()?;
    let first = response.results.first()?;
    Some((first.latitude, first.longitude))
}

#[cfg(test)]
fn parse_forecast(json: &str) -> Option<(i64, f64)> {
    let response: Response = serde_json::from_str(json).ok()?;
    Some((response.current.weather_code, response.current.wind_speed_10m))
}

fn geocode(locale: &str, endpoint: &str) -> Option<(f64, f64)> {
    let body = crate::infrastructure::http::agent()
        .get(endpoint)
        .set(USER_AGENT_HEADER, crate::infrastructure::http::USER_AGENT)
        .query(NAME_QUERY, locale)
        .query(COUNT_QUERY, SINGLE_RESULT)
        .call()
        .ok()?
        .into_string()
        .ok()?;
    parse_geocode(&body)
}

fn fetch_forecast(location: &Location) -> Option<Forecast> {
    let (lat, lon) = if location.locale.is_empty() {
        (location.lat, location.lon)
    } else {
        geocode(&location.locale, &location.geocode_endpoint)?
    };
    let body = crate::infrastructure::http::agent()
        .get(&location.endpoint)
        .set(USER_AGENT_HEADER, crate::infrastructure::http::USER_AGENT)
        .query(LATITUDE_QUERY, &format!("{lat:.4}"))
        .query(LONGITUDE_QUERY, &format!("{lon:.4}"))
        .query(CURRENT_QUERY, CURRENT_FIELDS)
        .query(TIME_FORMAT_QUERY, TIME_FORMAT_UNIX)
        .query(TIMEZONE_QUERY, TIMEZONE_AUTO)
        .call()
        .ok()?
        .into_string()
        .ok()?;
    let response: Response = serde_json::from_str(&body).ok()?;
    let current = response.current;
    Some(Forecast {
        weather: map_weather(current.weather_code, current.wind_speed_10m),
        observation: Some(Observation {
            latitude: lat,
            longitude: lon,
            utc_offset_seconds: response.utc_offset_seconds,
            observed_at: current.time.as_ref().and_then(serde_json::Value::as_i64),
            temperature: current.temperature_2m,
            cloud_cover: current.cloud_cover,
            snow_depth: current.snow_depth,
            is_day: current.is_day.map(|day| day != 0),
        }),
        stale: false,
        fetched_at: SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|time| time.as_secs()),
    })
}

pub(crate) fn refresh_interval() -> Duration {
    std::env::var(REFRESH_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0 && *seconds <= REFRESH.as_secs())
        .map_or(REFRESH, Duration::from_secs)
}

fn location(locale: &str, lat: f64, lon: f64) -> Option<Location> {
    if locale.trim().is_empty() && lat == 0.0 && lon == 0.0 {
        return None;
    }
    Some(Location {
        locale: locale.trim().to_owned(),
        lat: if locale.trim().is_empty() { lat } else { 0.0 },
        lon: if locale.trim().is_empty() { lon } else { 0.0 },
        endpoint: std::env::var(ENDPOINT_ENV).unwrap_or_else(|_| FORECAST_ENDPOINT.to_owned()),
        geocode_endpoint: std::env::var(GEOCODE_ENDPOINT_ENV)
            .unwrap_or_else(|_| GEOCODE_ENDPOINT.to_owned()),
    })
}

pub(crate) fn cached(locale: &str, lat: f64, lon: f64) -> Option<Forecast> {
    let location = location(locale, lat, lon)?;
    lock(&CACHE)
        .as_ref()
        .filter(|cached| cached.location == location)
        .map(|cached| cached.forecast.clone())
}

pub(crate) fn forecast(locale: &str, lat: f64, lon: f64, force: bool) -> Forecast {
    let Some(location) = location(locale, lat, lon) else { return Forecast::default() };
    let _fetch = lock(&FETCH_LOCK);
    if !force
        && let Some(cached) =
            lock(&CACHE).as_ref().filter(|cached| cached.matches(&location, Instant::now()))
    {
        return cached.forecast.clone();
    }
    let forecast = fetch_forecast(&location).unwrap_or_default();
    *lock(&CACHE) = Some(Cached { location, checked: Instant::now(), forecast: forecast.clone() });
    forecast
}

pub fn current(locale: &str, lat: f64, lon: f64) -> Vec<String> {
    forecast(locale, lat, lon, false).weather
}

#[cfg(test)]
mod tests;

pub(crate) fn outdoors_forecast(config: &skwd_wall_core::config::Config, force: bool) -> Forecast {
    let Some(wanted) = location(&config.locale(), config.latitude(), config.longitude()) else {
        return Forecast::default();
    };
    let path = std::path::PathBuf::from(config.cache_dir()).join(FORECAST_CACHE);
    let previous = cached(&config.locale(), config.latitude(), config.longitude())
        .filter(|forecast| !forecast.weather.is_empty())
        .or_else(|| {
            std::fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
                .filter(|saved| saved.location == wanted)
                .map(|saved| saved.forecast)
        });
    let mut current = forecast(&config.locale(), config.latitude(), config.longitude(), force);
    if current.weather.is_empty() {
        if let Some(mut previous) = previous {
            previous.stale = true;
            return previous;
        }
    } else {
        current.stale = false;
        if let Ok(bytes) =
            serde_json::to_vec(&Saved { location: wanted, forecast: current.clone() })
            && let Err(error) = std::fs::create_dir_all(config.cache_dir())
                .and_then(|()| skwd_config::atomic_write(&path, &bytes))
        {
            log::warn!("weather: forecast save failed: {error}");
        }
    }
    current
}
