use super::*;

#[test]
fn overview_pause_requires_confirmed_closed_state() {
    assert!(should_pause(true, Some(false)));
    assert!(!should_pause(true, Some(true)));
    assert!(!should_pause(true, None));
    assert!(!should_pause(false, Some(false)));
    assert_eq!(parse_open(r#"{"OverviewOpenedOrClosed":{"is_open":true}}"#), Some(true));
    assert_eq!(parse_open(r#"{"OverviewOpenedOrClosed":{"is_open":false}}"#), Some(false));
    assert_eq!(parse_open(r#"{"Ok":"Handled"}"#), None);
    assert_eq!(parse_open(r#"{"OverviewOpenedOrClosed":{"is_open":"false"}}"#), None);
}
