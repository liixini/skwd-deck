#![cfg(test)]

use super::*;

#[test]
fn bug_report_redacts_keys() {
    let config = Config::from_root(serde_json::json!({
        "wallhaven": {"apiKey": "WALLHAVEN_SECRET"},
        "steam": {"apiKey": "STEAM_SECRET"},
        "sources": {
            "pexels": {"apiKey": "PEXELS_SECRET"},
            "unsplash": {"accessKey": "UNSPLASH_SECRET"}
        }
    }));
    let report = concat!(
        "url=https://wallhaven.cc/api?apikey=WALLHAVEN_SECRET\n",
        "provider echoed STEAM_SECRET PEXELS_SECRET UNSPLASH_SECRET\n",
        "ordinary diagnostic remains visible"
    );
    let safe = redact_bug_report(report, &config);

    for secret in ["WALLHAVEN_SECRET", "STEAM_SECRET", "PEXELS_SECRET", "UNSPLASH_SECRET"] {
        assert!(!safe.contains(secret));
    }
    assert!(safe.contains("apikey=[REDACTED]"));
    assert!(safe.contains("ordinary diagnostic remains visible"));
}
