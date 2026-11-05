use super::*;

fn sample() -> StudioPalette {
    let mut ansi = [Rgb(0, 0, 0); ANSI_SLOTS];
    for (slot, color) in ansi.iter_mut().enumerate() {
        *color = Rgb(slot as u8 * 10, 20, 30);
    }
    StudioPalette {
        base: Rgb(1, 2, 3),
        panel: Rgb(4, 5, 6),
        raised: Rgb(7, 8, 9),
        text: Rgb(240, 241, 242),
        quiet: Rgb(150, 151, 152),
        accent: Rgb(250, 90, 80),
        on_accent: Rgb(20, 10, 10),
        second: Rgb(130, 180, 210),
        third: Rgb(170, 180, 210),
        ansi,
    }
}

#[test]
fn wire_round_trips() {
    let palette = sample();
    assert_eq!(StudioPalette::from_wire(&palette.to_wire()), Some(palette));
}

#[test]
fn wire_shape() {
    let wire = sample().to_wire();
    assert_eq!(wire["base"], "#010203");
    assert_eq!(wire["onAccent"], "#140a0a");
    assert_eq!(wire["ansi"][1], "#0a141e");
    assert!(StudioPalette::from_wire(&serde_json::json!({"base": "#000000"})).is_none());
}

#[test]
fn theme_shape() {
    let value = sample().theme_value();
    assert_eq!(value["primary"], "#fa5a50");
    assert_eq!(value["surface"], "#010203");
    assert_eq!(value["surfaceContainer"], "#040506");
    assert_eq!(value["surfaceVariant"], "#070809");
    assert_eq!(value["outline"], "#969798");
    assert_eq!(value["_scheme"]["colors"]["secondary"]["dark"]["color"], "#82b4d2");
    assert_eq!(value["ansi_red"], "#0a141e");
    assert_eq!(value["ansi_cyan_bright"], "#8c141e");
}

#[test]
fn role_keys() {
    for role in Role::ALL {
        assert_eq!(Role::from_key(role.key()), Some(role));
    }
}

#[test]
fn desktop_skips_terminal() {
    let palette = crate::studio::from_theme(&crate::preset("nord").expect("nord"));
    assert!(palette.desktop_value().get("ansi_red").is_none());
    assert!(palette.theme_value()["ansi_red"].is_string());
    assert_eq!(palette.desktop_value()["primary"], palette.theme_value()["primary"]);
}

#[test]
fn mix_meets_both_ends() {
    let from = crate::studio::from_theme(&crate::preset("nord").expect("nord"));
    let to = crate::studio::from_theme(&crate::preset("graphite-light").expect("graphite"));
    assert_eq!(from.mix(&to, 0.0), from);
    assert_eq!(from.mix(&to, 1.0), to);
    assert_ne!(from.mix(&to, 0.5).base, from.base);
}
