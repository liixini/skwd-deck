use super::*;

#[test]
fn decode_defaults_and_bounds() {
    assert_eq!(
        SearchParams::decode(&serde_json::json!({})),
        SearchParams {
            query: String::new(),
            query_type: 3,
            trend_days: 7,
            page: 1,
            required_tags: vec![],
            excluded_tags: vec![],
        }
    );
    let params = SearchParams::decode(&serde_json::json!({
        "query_type": 1, "days": 99, "page": 0,
        "tags": ["Landscape", 4, "", "Anime"],
        "excluded_tags": "nope"
    }));
    assert_eq!((params.query_type, params.trend_days, params.page), (1, 7, 1));
    assert_eq!(params.required_tags, vec!["Landscape", "", "Anime"]);
    assert!(params.excluded_tags.is_empty());
}
