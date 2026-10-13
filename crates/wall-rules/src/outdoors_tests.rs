use super::*;

#[test]
fn seasons_follow_hemisphere() {
    for (latitude, month, expected) in [
        (59.0, 7, OutdoorSeason::Summer),
        (59.0, 1, OutdoorSeason::Winter),
        (-34.0, 7, OutdoorSeason::Winter),
        (-34.0, 1, OutdoorSeason::Summer),
        (1.0, 7, OutdoorSeason::Neutral),
        (20.0, 1, OutdoorSeason::Neutral),
    ] {
        assert_eq!(season(latitude, month, None), expected);
    }
    assert_eq!(season(40.0, 1, Some(19.0)), OutdoorSeason::Neutral);
}

fn conditions(weather: &[String]) -> Conditions<'_> {
    Conditions {
        latitude: 59.3,
        longitude: 18.0,
        month: 7,
        day_of_year: 196,
        utc_minute: 720.0,
        temperature: Some(20.0),
        snow_depth: None,
        is_day: None,
        weather,
    }
}

#[test]
fn winter_needs_snow_evidence() {
    let weather = vec!["cloudy".into()];
    let mut conditions = conditions(&weather);
    conditions.month = 1;
    conditions.temperature = Some(-5.0);
    let context = Context::from_conditions(&conditions);
    assert_eq!(context.season, OutdoorSeason::Winter);
    assert!(!context.snow);
    conditions.snow_depth = Some(0.05);
    assert!(Context::from_conditions(&conditions).snow);
}

#[test]
fn daylight_uses_remote_longitude() {
    let mut conditions = conditions(&[]);
    conditions.longitude = 150.0;
    conditions.latitude = -34.0;
    assert_eq!(Context::from_conditions(&conditions).light, LightPhase::Night);
    conditions.utc_minute = 120.0;
    assert_eq!(Context::from_conditions(&conditions).light, LightPhase::Day);
}

#[test]
fn polar_day_and_night() {
    let mut conditions = conditions(&[]);
    conditions.latitude = 89.0;
    conditions.day_of_year = 172;
    assert_eq!(Context::from_conditions(&conditions).light, LightPhase::Day);
    conditions.day_of_year = 355;
    assert_eq!(Context::from_conditions(&conditions).light, LightPhase::Night);
    conditions.is_day = Some(true);
    assert_eq!(Context::from_conditions(&conditions).light, LightPhase::Day);
}

#[test]
fn queries_preserve_all_facets() {
    let context = Context::from_conditions(&conditions(&["cloudy".into()]));
    assert!(context.query().contains("summer"));
    assert!(context.query().contains("cloudy"));
    assert!(context.query().contains("daylight"));
    assert!(context.negative_query().contains("snow"));
}

#[test]
fn twilight_owns_next_deadline() {
    let mut conditions = conditions(&[]);
    let (rise, _) = crate::schedule::sun_times_utc_min(
        conditions.day_of_year,
        conditions.latitude,
        conditions.longitude,
    )
    .unwrap();
    conditions.utc_minute = rise;
    assert_eq!(Context::from_conditions(&conditions).light, LightPhase::Dawn);
    assert!((2100..=2102).contains(&next_light_seconds(&conditions)));
}

#[test]
fn boundary_advances_without_polling() {
    let mut conditions = conditions(&[]);
    let (rise, _) = crate::schedule::sun_times_utc_min(
        conditions.day_of_year,
        conditions.latitude,
        conditions.longitude,
    )
    .unwrap();
    conditions.utc_minute = rise + TWILIGHT_MINUTES - 0.01;
    let delay = next_light_seconds(&conditions);
    assert!(delay <= 2);
    conditions.utc_minute += delay as f64 / SECONDS_PER_MINUTE;
    assert_eq!(Context::from_conditions(&conditions).light, LightPhase::Day);
    assert!(next_light_seconds(&conditions) > 60);
}
