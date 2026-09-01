fn normalize_degrees(degrees: f64) -> f64 {
    degrees.rem_euclid(360.0)
}

pub fn sun_times_utc_min(day_of_year: u32, latitude: f64, longitude: f64) -> Option<(f64, f64)> {
    let zenith = 90.833_f64;
    let longitude_hour = longitude / 15.0;
    let calculate = |rising: bool| -> Option<f64> {
        let base = if rising { 6.0 } else { 18.0 };
        let days = day_of_year as f64 + (base - longitude_hour) / 24.0;
        let anomaly = 0.9856 * days - 3.289;
        let sun_longitude = normalize_degrees(
            anomaly
                + 1.916 * anomaly.to_radians().sin()
                + 0.020 * (2.0 * anomaly).to_radians().sin()
                + 282.634,
        );
        let mut right_ascension =
            normalize_degrees(sun_longitude.to_radians().tan().atan().to_degrees());
        let longitude_quadrant = (sun_longitude / 90.0).floor() * 90.0;
        let ascension_quadrant = (right_ascension / 90.0).floor() * 90.0;
        right_ascension = (right_ascension + (longitude_quadrant - ascension_quadrant)) / 15.0;
        let sin_declination = 0.39782 * sun_longitude.to_radians().sin();
        let cos_declination = sin_declination.asin().cos();
        let cos_hour_angle = (zenith.to_radians().cos()
            - sin_declination * latitude.to_radians().sin())
            / (cos_declination * latitude.to_radians().cos());
        if !(-1.0..=1.0).contains(&cos_hour_angle) {
            return None;
        }
        let hour_angle = if rising {
            360.0 - cos_hour_angle.acos().to_degrees()
        } else {
            cos_hour_angle.acos().to_degrees()
        } / 15.0;
        let local_mean_time = hour_angle + right_ascension - 0.06571 * days - 6.622;
        let utc_hours = (local_mean_time - longitude_hour).rem_euclid(24.0);
        Some(utc_hours * 60.0)
    };
    Some((calculate(true)?, calculate(false)?))
}

#[cfg(test)]
#[path = "sun_tests.rs"]
mod tests;
