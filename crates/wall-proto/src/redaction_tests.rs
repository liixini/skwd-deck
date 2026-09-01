use super::*;

#[test]
fn redacts_query_credentials() {
    let text = "request https://example.test/search?q=forest&apikey=WH_SECRET&page=2 failed";
    let safe = redact_sensitive(text);
    assert_eq!(
        safe,
        "request https://example.test/search?q=forest&apikey=[REDACTED]&page=2 failed"
    );
    assert!(!safe.contains("WH_SECRET"));
}

#[test]
fn redacts_credential_spellings() {
    let text = "api_key=nasa X-API-Key:pexels accessToken=u password=p client_secret=c";
    assert_eq!(
        redact_sensitive(text),
        "api_key=[REDACTED] X-API-Key:[REDACTED] accessToken=[REDACTED] password=[REDACTED] client_secret=[REDACTED]"
    );
}

#[test]
fn redacts_json_headers() {
    let text = r#"{"apiKey":"abc123","other":"visible"} Authorization: Bearer bearer-secret"#;
    let safe = redact_sensitive(text);
    assert_eq!(
        safe,
        r#"{"apiKey":"[REDACTED]","other":"visible"} Authorization: Bearer [REDACTED]"#
    );
}

#[test]
fn known_secrets_min_length() {
    let safe = redact_known_secrets(
        "remote echoed LONG_SECRET; missing key; ordinary monkey=value",
        &["LONG_SECRET", "key", "x"],
    );
    assert_eq!(safe, "remote echoed [REDACTED]; missing key; ordinary monkey=value");
}

#[test]
fn multiple_credentials_one_url() {
    let safe = redact_sensitive("https://x.test/?key=steam&token=next#fragment");
    assert_eq!(safe, "https://x.test/?key=[REDACTED]&token=[REDACTED]#fragment");
}
