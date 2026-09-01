use super::*;

fn range(start: &str, end: &str) -> DateRange {
    DateRange { start: parse_date(start).unwrap(), end: parse_date(end).unwrap() }
}

#[test]
fn date_parsing() {
    assert_eq!(parse_date("12-25"), Some(DateSpec { year: None, month: 12, day: 25 }));
    assert_eq!(parse_date("2026-12-25"), Some(DateSpec { year: Some(2026), month: 12, day: 25 }));
    assert_eq!(parse_date(" 01 - 01 "), Some(DateSpec { year: None, month: 1, day: 1 }));
    assert_eq!(parse_date("13-01"), None);
    assert_eq!(parse_date("12-32"), None);
    assert_eq!(parse_date("2026"), None);
}

#[test]
fn ranges_wrap_new_year() {
    assert!(date_in_range(&range("12-25", "12-25"), 2026, 12, 25));
    assert!(!date_in_range(&range("12-25", "12-25"), 2026, 12, 24));
    assert!(date_in_range(&range("2030-01-01", "2030-01-01"), 2030, 1, 1));
    assert!(!date_in_range(&range("2030-01-01", "2030-01-01"), 2031, 1, 1));
    let holidays = range("12-20", "01-05");
    assert!(date_in_range(&holidays, 2026, 12, 25));
    assert!(date_in_range(&holidays, 2027, 1, 2));
    assert!(!date_in_range(&holidays, 2026, 6, 1));
}
