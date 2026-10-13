macro_rules! photographs {
    ($original:literal, $thumbnail:literal; $($stem:literal),+ $(,)?) => {
        &[$(
            (concat!($stem, $original), include_bytes!(concat!($stem, $original))),
            (concat!($stem, $thumbnail), include_bytes!(concat!($stem, $thumbnail))),
        )+]
    };
}

pub(super) const IMAGES: &[(&str, &[u8])] = photographs!(".jpg", ".thumb.jpg";
    "winter_frost",
    "winter_lake",
    "summer_overcast_nps",
    "summer_clear_nps",
    "ambient_day_nps",
    "ambient_night_nps_3840",
    "ambient_rain_nps",
    "spring_pond_nps",
    "autumn_pond_nps",
    "autumn_bear_nps",
    "winter_nosnow_nps",
    "winter_overcast_pond",
);
