#![cfg(test)]

use super::*;

#[test]
fn query_pairs_shape() {
    let params = SearchParams {
        query: "anime".into(),
        query_type: 3,
        days: 30,
        tags: vec!["Scene".into(), String::new(), "1080p".into()],
        excluded_tags: vec!["NSFW".into(), String::new()],
        page: 2,
        numperpage: 24,
    };
    let pairs = query_pairs(&params);
    let has = |key: &str, val: &str| pairs.iter().any(|(name, value)| name == key && value == val);
    assert!(has("appid", "431960"));
    assert!(has("query_type", "3"));
    assert!(has("page", "2"));
    assert!(has("numperpage", "24"));
    assert!(has("match_all_tags", "true"));
    assert!(has("search_text", "anime"));
    assert!(has("days", "7"));
    assert!(has("requiredtags[0]", "Scene"));
    assert!(has("requiredtags[1]", "1080p"));
    assert!(has("excludedtags[0]", "NSFW"));
    assert!(!pairs.iter().any(|(_, value)| value.is_empty()));
}

#[test]
fn days_only_for_trend() {
    let params = SearchParams { query_type: 0, days: 30, ..Default::default() };
    let pairs = query_pairs(&params);
    assert!(!pairs.iter().any(|(key, _)| key == "days"));
}

#[test]
fn parse_search_basic() {
    let json = r#"{"response":{"total":61,"publishedfiledetails":[
            {"publishedfileid":"123","title":"Forest","preview_url":"https://x/p.jpg","file_size":"2048","subscriptions":99,"tags":[{"tag":"Scene"},{"tag":"Nature"}]},
            {"publishedfileid":"0","title":"invalid"},
            {"publishedfileid":"456","title":"City","preview_url":"https://x/c.jpg","file_size":"bad"}
        ]}}"#;
    let page = parse_search(json, 1, 30).unwrap();
    assert_eq!(page.results.len(), 1);
    assert_eq!(page.results[0].id, "123");
    assert_eq!(page.results[0].file_size, 2048);
    assert_eq!(page.results[0].tags, "Scene, Nature");
    assert_eq!(page.last_page, 3);
    assert_eq!(page.current_page, 1);
}

#[test]
fn parse_search_empty() {
    assert!(parse_search("{}", 1, 30).is_err());
}

#[test]
fn helper_search_paging() {
    let json = r#"{"results":[
            {"id":"123","title":"Forest","preview_url":"https://x/p.jpg","file_size":2048,"subscriptions":99,"tags":"Scene, Nature"},
            {"id":"","title":"junk"}
        ]}"#;
    let page = parse_helper_search(json, 2).unwrap();
    assert_eq!(page.results.len(), 1);
    assert_eq!(page.results[0].id, "123");
    assert_eq!(page.results[0].subscriptions, 99);
    assert_eq!(page.last_page, 3);

    let empty = parse_helper_search(r#"{"results":[]}"#, 4).unwrap();
    assert_eq!(empty.last_page, 4);

    assert!(parse_helper_search(r#"{"error":"Steam is not running"}"#, 1).is_err());

    let unsupported = r#"{"results":[{"id":"9","tags":"Web, Anime"}]}"#;
    assert!(parse_helper_search(unsupported, 1).unwrap().results.is_empty());
}

#[test]
fn workshop_id_extract() {
    let ids = vec!["1234567".to_string(), "7654321".to_string()];
    let done = std::collections::HashSet::new();
    assert_eq!(
        extract_workshop_id("Downloading item 1234567 ...", &ids, &done),
        Some("1234567".to_string())
    );
    let mut done2 = std::collections::HashSet::new();
    done2.insert("1234567".to_string());
    assert_eq!(extract_workshop_id("Success. Downloaded item 1234567", &ids, &done2), None);
    assert_eq!(extract_workshop_id("Downloading item 9999999", &ids, &done), None);
    assert_eq!(extract_workshop_id("Connecting anonymously", &ids, &done), None);
}

#[test]
fn percent_token() {
    assert_eq!(extract_percent("downloading, 42.50% (1234 / 5678)"), Some(0.425));
    assert_eq!(extract_percent(" 100% done"), Some(1.0));
    assert_eq!(
        extract_percent("Update state (0x61) downloading, progress: 42.50 (1234 / 5678)"),
        None
    );
    assert_eq!(extract_percent("no percent here"), None);
}

#[test]
fn steamcmd_args_shape() {
    let ids = vec!["111".to_string(), "222".to_string()];
    let args = steamcmd_args("user", "/steam", &ids);
    assert_eq!(
        args,
        vec![
            "+force_install_dir",
            "/steam",
            "+login",
            "user",
            "+workshop_download_item",
            "431960",
            "111",
            "+workshop_download_item",
            "431960",
            "222",
            "+quit",
        ]
    );
    let anon = steamcmd_args("", "", &["5".to_string()]);
    assert_eq!(
        anon,
        vec!["+login", "anonymous", "+workshop_download_item", "431960", "5", "+quit"]
    );
}

#[test]
fn auth_error_detect() {
    assert!(is_auth_error("Cached credentials not found"));
    assert!(is_auth_error("FAILED (Login Failure)"));
    assert!(!is_auth_error("Downloading item 123"));
}

#[test]
fn ext_from_url_query() {
    assert_eq!(ext_from_url("https://steamuserimages.akamai.net/ugc/abc.png?w=100"), "png");
    assert_eq!(ext_from_url("https://x/y/preview"), "jpg");
}
