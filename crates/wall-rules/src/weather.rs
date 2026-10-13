use wall_proto::weather_category;

const WIND_THRESHOLD_KMH: f64 = 30.0;
const CLEAR_CODES: &[i64] = &[0, 1];
const CLOUDY_CODES: &[i64] = &[2, 3];
const FOG_CODES: &[i64] = &[45, 48];
const RAIN_CODES: &[i64] = &[51, 53, 55, 56, 57, 61, 63, 65, 66, 67, 80, 81, 82];
const SNOW_CODES: &[i64] = &[71, 73, 75, 77, 85, 86];
const STORM_CODES: &[i64] = &[95, 96, 99];
const CODES: &[(&[i64], &str)] = &[
    (CLEAR_CODES, weather_category::CLEAR),
    (CLOUDY_CODES, weather_category::CLOUDY),
    (FOG_CODES, weather_category::FOGGY),
    (RAIN_CODES, weather_category::RAINY),
    (SNOW_CODES, weather_category::SNOWY),
    (STORM_CODES, weather_category::STORMY),
];

pub fn map_weather(code: i64, wind_kmh: f64) -> Vec<String> {
    let Some((_, category)) = CODES.iter().find(|(codes, _)| codes.contains(&code)) else {
        return Vec::new();
    };
    let mut weather = vec![(*category).to_owned()];
    if code == 0 {
        weather.push(weather_category::SUNNY.to_owned());
    }
    if wind_kmh >= WIND_THRESHOLD_KMH {
        weather.push(weather_category::WINDY.to_owned());
    }
    weather
}

#[cfg(test)]
#[path = "weather_tests.rs"]
mod tests;
