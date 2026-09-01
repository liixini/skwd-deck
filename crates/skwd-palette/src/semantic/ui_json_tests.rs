use super::*;
use crate::Rgb;

#[test]
fn ui_role_names() {
    let semantic = Semantic {
        bg: Rgb(28, 36, 44),
        surface: Rgb(42, 50, 58),
        fg: Rgb(220, 225, 230),
        dim: Rgb(120, 130, 140),
        accent: Rgb(196, 92, 60),
    };
    let value = ui_palette(&semantic);
    assert_eq!(value["surface"], semantic.bg.hex());
    assert_eq!(value["surfaceText"], semantic.fg.hex());
    assert_eq!(value["primary"], semantic.accent.hex());
    for role in ["surfaceVariant", "surfaceContainer", "background", "tertiary"] {
        assert!(value.get(role).and_then(serde_json::Value::as_str).is_some());
    }
}
