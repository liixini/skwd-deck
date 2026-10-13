use super::*;

#[test]
fn status_round_trip() {
    let json = serde_json::json!({
        "weather": ["rainy"], "enabled": true, "status": "active", "category": "rainy",
        "source": "playlists", "selection": "automatic", "query": null,
        "playlist_id": 7, "playlist_name": "Rain", "member_keys": ["static:rain.png"],
        "resolved_count": 1, "fetched_at": 1_791_700_000, "paused_outputs": [], "resume_available": false
    });
    let status: WeatherStatus = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(status.status, WeatherState::Active);
    assert_eq!(serde_json::to_value(status).unwrap(), json);
}

#[test]
fn legacy_response_decodes() {
    let status: WeatherStatus =
        serde_json::from_value(serde_json::json!({"weather": ["clear"]})).unwrap();
    assert_eq!(status.weather, ["clear"]);
    assert!(!status.enabled);
    assert!(!status.resume_available);
    assert!(status.fetched_at.is_none());
    assert_eq!(status.source, WeatherSource::Playlists);
    assert_eq!(status.selection, WeatherSelection::Automatic);
    assert!(status.query.is_none());
}

#[test]
fn semantic_status_round_trips() {
    let status = WeatherStatus {
        source: WeatherSource::Semantic,
        query: Some("misty mountains".into()),
        status: WeatherState::SearchUnavailable,
        ..WeatherStatus::default()
    };
    let json = serde_json::to_value(&status).unwrap();
    assert_eq!(json["source"], "semantic");
    assert_eq!(json["status"], "search_unavailable");
    assert_eq!(serde_json::from_value::<WeatherStatus>(json).unwrap(), status);
}

#[test]
fn weather_wire_names() {
    assert_eq!(crate::rpc::WALL_WEATHER_CANDIDATES, "wall.weather.candidates");
    assert_eq!(crate::rpc::WALL_WEATHER_REFRESH, "wall.weather.refresh");
    assert_eq!(crate::rpc::WALL_WEATHER_RESUME, "wall.weather.resume");
    assert_eq!(crate::ev::WEATHER_CHANGED, "skwd.wall.weather_changed");
    assert!(crate::CAPABILITIES.contains(&"weather-playlists"));
}

#[test]
fn candidates_round_trip() {
    let result = WeatherCandidates {
        category: "rainy".into(),
        query: Some("rain on street".into()),
        member_keys: vec!["static:rain.png".into()],
        error: None,
    };
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["category"], "rainy");
    assert_eq!(serde_json::from_value::<WeatherCandidates>(json).unwrap(), result);
    assert_eq!(serde_json::to_value(WeatherSelection::Approved).unwrap(), "approved");
}

#[test]
fn outdoors_request_shapes() {
    use super::{WeatherExcludeRequest, WeatherFollowRequest, WeatherOutputRequest};
    let follow = WeatherFollowRequest { enabled: true };
    let output = WeatherOutputRequest { output: "DP-1".into() };
    let exclude = WeatherExcludeRequest { key: "static:one.jpg".into() };
    assert_eq!(serde_json::to_value(&follow).unwrap(), serde_json::json!({"enabled":true}));
    assert_eq!(serde_json::to_value(&output).unwrap(), serde_json::json!({"output":"DP-1"}));
    assert_eq!(
        serde_json::to_value(&exclude).unwrap(),
        serde_json::json!({"key":"static:one.jpg"})
    );
    assert_eq!(
        serde_json::from_value::<WeatherFollowRequest>(serde_json::to_value(&follow).unwrap())
            .unwrap(),
        follow
    );
    assert_eq!(
        serde_json::from_value::<WeatherOutputRequest>(serde_json::to_value(&output).unwrap())
            .unwrap(),
        output
    );
    assert_eq!(
        serde_json::from_value::<WeatherExcludeRequest>(serde_json::to_value(&exclude).unwrap())
            .unwrap(),
        exclude
    );
}
