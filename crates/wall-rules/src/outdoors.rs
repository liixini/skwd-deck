use serde::{Deserialize, Serialize};
use wall_proto::{LightPhase, OutdoorSeason, weather_category};

const TROPICAL_LATITUDE: f64 = 23.44;
const WARM_WINTER_CELSIUS: f64 = 16.0;
const SNOW_DEPTH_METRES: f64 = 0.01;
const MONTHS_PER_YEAR: u32 = 12;
const SEASON_MONTHS: u32 = 3;
const SOUTHERN_SHIFT: u32 = 6;
const MINUTES_PER_DAY: f64 = 1440.0;
const SECONDS_PER_MINUTE: f64 = 60.0;
const TWILIGHT_MINUTES: f64 = 35.0;
const SUMMER_SOLSTICE_DAY: f64 = 172.0;
const DAYS_PER_YEAR: f64 = 365.25;
const SEASON_WORDS: &[(OutdoorSeason, &str)] = &[
    (OutdoorSeason::Neutral, "natural landscape"),
    (OutdoorSeason::Spring, "spring landscape fresh foliage"),
    (OutdoorSeason::Summer, "summer landscape green foliage"),
    (OutdoorSeason::Autumn, "autumn landscape muted foliage"),
    (OutdoorSeason::Winter, "winter landscape bare trees muted vegetation"),
];
const LIGHT_WORDS: &[(LightPhase, &str)] = &[
    (LightPhase::Dawn, "soft dawn light"),
    (LightPhase::Day, "daylight"),
    (LightPhase::Dusk, "soft dusk twilight"),
    (LightPhase::Night, "night darkness moonlight"),
];
const NO_SNOW: &str = "snow covered ground blizzard ice";
const NO_DAYLIGHT: &str = "bright sunny midday blue daylight sky";
const GROUND_SNOW: &str = "snow covered ground";
const WEATHER_OPPOSITES: &[(&str, &str)] = &[
    (weather_category::CLOUDY, "clear blue sky bright direct sunshine"),
    (weather_category::RAINY, "dry arid desert bright sunshine"),
    (weather_category::SNOWY, "lush green tropical landscape"),
    (weather_category::STORMY, "calm clear sky"),
    (weather_category::FOGGY, "clear distant view crisp blue sky"),
];
const NO_NIGHT: &str = "night darkness stars moonlight";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Context {
    pub season: OutdoorSeason,
    pub light: LightPhase,
    pub category: String,
    pub snow: bool,
}

pub struct Conditions<'a> {
    pub latitude: f64,
    pub longitude: f64,
    pub month: u32,
    pub day_of_year: u32,
    pub utc_minute: f64,
    pub temperature: Option<f64>,
    pub snow_depth: Option<f64>,
    pub is_day: Option<bool>,
    pub weather: &'a [String],
}

impl Context {
    pub fn from_conditions(conditions: &Conditions<'_>) -> Self {
        let category = weather_category::PRIORITY
            .iter()
            .find(|category| conditions.weather.iter().any(|item| item == **category))
            .copied()
            .unwrap_or(weather_category::CLOUDY)
            .to_owned();
        Self {
            season: season(conditions.latitude, conditions.month, conditions.temperature),
            light: light(conditions),
            snow: category == weather_category::SNOWY
                || conditions.snow_depth.is_some_and(|depth| depth >= SNOW_DEPTH_METRES),
            category,
        }
    }

    pub fn query(&self) -> String {
        let season =
            SEASON_WORDS.iter().find(|(season, _)| *season == self.season).expect("season query").1;
        let light =
            LIGHT_WORDS.iter().find(|(light, _)| *light == self.light).expect("light query").1;
        let snow = if self.snow { GROUND_SNOW } else { "" };
        format!("{season}, {} weather, {light}, {snow}", self.category)
    }

    pub fn negative_query(&self) -> String {
        let light = if self.light == LightPhase::Night { NO_DAYLIGHT } else { NO_NIGHT };
        let weather = WEATHER_OPPOSITES
            .iter()
            .find(|(category, _)| *category == self.category)
            .map_or("", |(_, query)| *query);
        let snow = if self.snow { "" } else { NO_SNOW };
        format!("{light}, {weather}, {snow}")
    }
}

pub fn season(latitude: f64, month: u32, temperature: Option<f64>) -> OutdoorSeason {
    if latitude.abs() <= TROPICAL_LATITUDE {
        return OutdoorSeason::Neutral;
    }
    let month =
        if latitude < 0.0 { (month + SOUTHERN_SHIFT - 1) % MONTHS_PER_YEAR + 1 } else { month };
    let season = match month % MONTHS_PER_YEAR / SEASON_MONTHS {
        0 => OutdoorSeason::Winter,
        1 => OutdoorSeason::Spring,
        2 => OutdoorSeason::Summer,
        _ => OutdoorSeason::Autumn,
    };
    if season == OutdoorSeason::Winter
        && temperature.is_some_and(|temp| temp >= WARM_WINTER_CELSIUS)
    {
        OutdoorSeason::Neutral
    } else {
        season
    }
}

fn light(conditions: &Conditions<'_>) -> LightPhase {
    let Some((rise, set)) = crate::schedule::sun_times_utc_min(
        conditions.day_of_year,
        conditions.latitude,
        conditions.longitude,
    ) else {
        let northern_summer = ((f64::from(conditions.day_of_year) - SUMMER_SOLSTICE_DAY)
            / DAYS_PER_YEAR
            * std::f64::consts::TAU)
            .cos()
            >= 0.0;
        return if conditions.is_day.unwrap_or(northern_summer == (conditions.latitude >= 0.0)) {
            LightPhase::Day
        } else {
            LightPhase::Night
        };
    };
    let near = |boundary: f64| {
        ((conditions.utc_minute - boundary + MINUTES_PER_DAY / 2.0).rem_euclid(MINUTES_PER_DAY)
            - MINUTES_PER_DAY / 2.0)
            .abs()
            <= TWILIGHT_MINUTES
    };
    if near(rise) {
        return LightPhase::Dawn;
    }
    if near(set) {
        return LightPhase::Dusk;
    }
    if (conditions.utc_minute - rise).rem_euclid(MINUTES_PER_DAY)
        < (set - rise).rem_euclid(MINUTES_PER_DAY)
    {
        LightPhase::Day
    } else {
        LightPhase::Night
    }
}

#[cfg(test)]
#[path = "outdoors_tests.rs"]
mod tests;

pub fn next_light_seconds(conditions: &Conditions<'_>) -> u64 {
    let Some((rise, set)) = crate::schedule::sun_times_utc_min(
        conditions.day_of_year,
        conditions.latitude,
        conditions.longitude,
    ) else {
        return (MINUTES_PER_DAY * SECONDS_PER_MINUTE) as u64;
    };
    [
        rise - TWILIGHT_MINUTES,
        rise + TWILIGHT_MINUTES,
        set - TWILIGHT_MINUTES,
        set + TWILIGHT_MINUTES,
    ]
    .iter()
    .map(|boundary| {
        ((boundary - conditions.utc_minute).rem_euclid(MINUTES_PER_DAY) * SECONDS_PER_MINUTE).ceil()
            as u64
            + 1
    })
    .min()
    .expect("solar boundaries")
}
