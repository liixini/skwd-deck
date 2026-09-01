use super::*;

fn assert_near(got: f64, want: f64, tolerance: f64, label: &str) {
    assert!((got - want).abs() <= tolerance, "{label}: {got} vs {want}");
}

#[test]
fn sun_times_almanac() {
    let (sunrise, sunset) = sun_times_utc_min(172, 51.5, -0.13).expect("london midsummer");
    assert_near(sunrise, 223.14, 3.0, "london sunrise");
    assert_near(sunset, 1221.40, 3.0, "london sunset");

    let (sunrise, sunset) = sun_times_utc_min(355, 40.7, -74.0).expect("nyc winter");
    assert_near(sunrise, 736.68, 3.0, "nyc sunrise");
    assert_near(sunset, 1291.85, 3.0, "nyc sunset");

    let (sunrise, sunset) = sun_times_utc_min(1, 0.0, 0.0).expect("equator");
    assert_near(sunrise, 356.16, 3.0, "equator sunrise");
    assert_near(sunset, 1083.47, 3.0, "equator sunset");
}

#[test]
fn polar_night_no_sun() {
    assert!(sun_times_utc_min(355, 80.0, 20.0).is_none());
}
