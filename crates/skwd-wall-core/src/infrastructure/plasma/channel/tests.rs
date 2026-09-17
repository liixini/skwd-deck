use super::{entry, publish, subscribe, subscribed};

fn names(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn every_output_needs_a_live_subscription() {
    let outputs = names(&["CHANNEL-A1", "CHANNEL-A2"]);
    assert!(!subscribed(outputs.iter()));
    let first = subscribe("CHANNEL-A1");
    assert!(!subscribed(outputs.iter()));
    let second = subscribe("CHANNEL-A2");
    let duplicate = subscribe("CHANNEL-A2");
    assert!(subscribed(outputs.iter()));
    drop(second);
    assert!(subscribed(outputs.iter()));
    drop(duplicate);
    assert!(!subscribed(outputs.iter()));
    drop(first);
    assert!(!subscribed(names(&["CHANNEL-A1"]).iter()));
}

#[test]
fn an_empty_output_set_is_never_subscribed() {
    let _subscription = subscribe("CHANNEL-B1");
    assert!(!subscribed(Vec::<String>::new().iter()));
}

#[test]
fn published_entries_replace_only_their_outputs() {
    let first = serde_json::json!({"CHANNEL-C1": {"paper": "a"}, "CHANNEL-C2": {"paper": "b"}});
    publish(first.as_object().unwrap());
    let second = serde_json::json!({"CHANNEL-C2": {"paper": "c"}});
    publish(second.as_object().unwrap());
    assert_eq!(entry("CHANNEL-C1").unwrap()["paper"], "a");
    assert_eq!(entry("CHANNEL-C2").unwrap()["paper"], "c");
    assert!(entry("CHANNEL-C3").is_none());
}
