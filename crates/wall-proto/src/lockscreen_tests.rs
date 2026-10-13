use super::*;

#[test]
fn set_wire_shape() {
    let request = LockScreenSetRequest {
        target: LockScreenTarget::Sddm,
        enabled: true,
        sddm_image: Some("/run/skwd.png".into()),
    };
    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(
        value,
        serde_json::json!({"target":"sddm","enabled":true,"sddm_image":"/run/skwd.png"})
    );
    assert_eq!(
        serde_json::from_value::<LockScreenSetRequest>(value).unwrap().target,
        request.target
    );
}

#[test]
fn result_wire_shape() {
    let result =
        LockScreenSetResult { ok: true, target: LockScreenTarget::Hyprlock, enabled: false };
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(value, serde_json::json!({"ok":true,"target":"hyprlock","enabled":false}));
    assert_eq!(serde_json::from_value::<LockScreenSetResult>(value).unwrap().target, result.target);
}

#[test]
fn sddm_choices_shape() {
    let choices = SddmChoices { current: "breeze".into(), themes: vec!["breeze".into()] };
    let value = serde_json::to_value(&choices).unwrap();
    assert_eq!(value, serde_json::json!({"current":"breeze","themes":["breeze"]}));
    assert_eq!(serde_json::from_value::<SddmChoices>(value).unwrap().current, choices.current);
}
