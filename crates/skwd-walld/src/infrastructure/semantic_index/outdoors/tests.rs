use super::*;

#[test]
fn rejects_unrelated_night_result() {
    let context = Context {
        season: OutdoorSeason::Summer,
        light: LightPhase::Night,
        category: weather_category::CLEAR.into(),
        snow: false,
    };
    assert!(!fits(&context, 0.005, |facet| match facet {
        Facet::Nature => -0.023,
        Facet::Other => 0.083,
        Facet::Night => 0.038,
        Facet::Day => 0.029,
        _ => 0.0,
    }));
}

#[test]
fn night_landscape_excludes_daylight() {
    let context = Context {
        season: OutdoorSeason::Summer,
        light: LightPhase::Night,
        category: weather_category::CLEAR.into(),
        snow: false,
    };
    let scores = [
        0.083, -0.003, 0.098, 0.061, 0.039, 0.049, 0.035, 0.009, 0.049, 0.064, 0.045, 0.0, 0.0, 0.0,
    ];
    assert!(fits(&context, 0.005, |facet| scores[facet as usize]));
    let day = Context { light: LightPhase::Day, ..context };
    assert!(!fits(&day, 0.005, |facet| scores[facet as usize]));
}

#[test]
fn snow_requires_observed_evidence() {
    let context = Context {
        season: OutdoorSeason::Winter,
        light: LightPhase::Day,
        category: weather_category::CLOUDY.into(),
        snow: true,
    };
    let scores = [0.068, 0.014, 0.015, 0.087, 0.125, 0.089, 0.043, 0.023, 0.073, 0.065, 0.023];
    assert!(fits(&context, 0.005, |facet| scores[facet as usize]));
    let warm = Context { snow: false, season: OutdoorSeason::Summer, ..context };
    assert!(!fits(&warm, 0.005, |facet| scores[facet as usize]));
}

#[test]
fn winter_excludes_summer_foliage() {
    let context = Context {
        season: OutdoorSeason::Winter,
        light: LightPhase::Day,
        category: weather_category::CLEAR.into(),
        snow: false,
    };
    let bare = [0.059, 0.005, 0.010, 0.073, 0.047, 0.115, 0.046, 0.038, 0.042, 0.058, 0.083];
    assert!(fits(&context, 0.005, |facet| bare[facet as usize]));
    let green = [0.101, -0.007, 0.016, 0.092, 0.054, 0.079, 0.097, 0.008, 0.046, 0.092, 0.114];
    assert!(!fits(&context, 0.005, |facet| green[facet as usize]));
}

#[test]
fn snowfall_rejects_clear_sky() {
    let context = Context {
        season: OutdoorSeason::Winter,
        light: LightPhase::Day,
        category: weather_category::SNOWY.into(),
        snow: true,
    };
    let clear = [0.069, 0.017, 0.016, 0.080, 0.111, 0.081, 0.049, 0.029, 0.046, 0.071, 0.035];
    assert!(!fits(&context, 0.005, |facet| clear[facet as usize]));
    let cloudy = [0.068, 0.014, 0.015, 0.087, 0.125, 0.089, 0.043, 0.023, 0.073, 0.065, 0.023];
    assert!(fits(&context, 0.005, |facet| cloudy[facet as usize]));
}

const CLOUDY_SCORES: [f32; FACETS.len()] =
    [0.2, 0.0, 0.0, 0.2, 0.0, 0.0, 0.2, 0.0, 0.2, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];

fn summer_clouds() -> Context {
    Context {
        season: OutdoorSeason::Summer,
        light: LightPhase::Day,
        category: weather_category::CLOUDY.into(),
        snow: false,
    }
}

#[test]
fn indexed_facets_follow_keys() {
    let keys = ["second", "first", "missing", "night"];
    let mut scores = Scores::default();
    for generation in 0..=FACETS.len() {
        let mut matches = Vec::new();
        for index in 0..keys.len() {
            let key = keys[(index + generation) % keys.len()];
            if generation == Facet::Other as usize + 1 && key == "missing" {
                continue;
            }
            let score = if generation == 0 {
                0.0
            } else if generation == Facet::Night as usize + 1 && key == "night" {
                0.3
            } else {
                CLOUDY_SCORES[generation - 1]
            };
            matches.push(skwd_lens_proto::Match { rank: index + 1, key: key.into(), score });
        }
        scores.append(SearchResponse::from_matches(generation as u64, matches, 0.0, 0.0));
    }
    assert!(scores.facets["missing"][Facet::Other as usize].is_nan());
    assert_eq!(scores.select(&summer_clouds(), 0.005), ["second", "first"]);
}

#[test]
fn facet_index_scales() {
    for count in [1_000, 10_000, 100_000] {
        let started = std::time::Instant::now();
        let mut scores = Scores::default();
        for generation in 0..=FACETS.len() {
            let matches = (0..count)
                .map(|index| skwd_lens_proto::Match {
                    rank: index + 1,
                    key: format!(
                        "static:library/outdoors/{:08}/landscape.jpg",
                        (index + generation) % count
                    ),
                    score: if generation == 0 { 0.0 } else { CLOUDY_SCORES[generation - 1] },
                })
                .collect();
            scores.append(SearchResponse::from_matches(generation as u64, matches, 0.0, 0.0));
        }
        assert_eq!(scores.facets.len(), count);
        assert_eq!(scores.select(&summer_clouds(), 0.005).len(), count);
        eprintln!("facet rows={count} facets={} elapsed={:?}", FACETS.len(), started.elapsed());
    }
}
