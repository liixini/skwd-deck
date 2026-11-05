use super::*;

const LABELS: &str = r#"{"days":["MON","TUE","WED","THU","FRI","SAT","SUN"],"day_names":["MONDAY","TUESDAY","WEDNESDAY","THURSDAY","FRIDAY","SATURDAY","SUNDAY"],"months":["JAN","FEB","MAR","APR","MAY","JUN","JUL","AUG","SEP","OCT","NOV","DEC"],"month_names":["JANUARY","FEBRUARY","MARCH","APRIL","MAY","JUNE","JULY","AUGUST","SEPTEMBER","OCTOBER","NOVEMBER","DECEMBER"]}"#;

#[test]
fn desk_wire_shape() {
    let wire = format!(
        r#"{{"scene":{{"widgets":[],"palette":{{"text":[1,2,3],"accent":[4,5,6],"shade":[7,8,9]}},"labels":{LABELS}}},"outputs":[{{"name":"DP-1","path":"/w/a.png","fill":"fit","background":{{"color":[18,52,86],"blur":true}}}}]}}"#
    );
    let desk: Desk = serde_json::from_str(&wire).unwrap();
    assert_eq!(desk.outputs[0].name, "DP-1");
    assert_eq!(desk.outputs[0].fill, "fit");
    assert_eq!(desk.outputs[0].background, Background { color: [18, 52, 86], blur: true });
    assert_eq!(serde_json::to_string(&desk).unwrap(), wire);
}

#[test]
fn editing_wire_shape() {
    assert_eq!(serde_json::to_string(&Editing { active: true }).unwrap(), r#"{"active":true}"#);
    assert!(serde_json::from_str::<Editing>("{}").is_err());
}
