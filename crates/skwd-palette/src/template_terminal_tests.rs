use super::*;
use crate::parse_hex;

#[test]
fn modes_stay_in_band() {
    let primary = parse_hex("#5e81ac").expect("hex");
    let dark = template_terminal(primary, true);
    let light = template_terminal(primary, false);
    for (normal, bright) in dark.normal.iter().zip(dark.bright) {
        assert!(to_hsl(*normal).2 >= DARK_LIGHT.0 - 0.01);
        assert!(to_hsl(bright).2 > to_hsl(*normal).2);
    }
    for (normal, bright) in light.normal.iter().zip(light.bright) {
        assert!(to_hsl(*normal).2 <= LIGHT_LIGHT.1 + 0.01);
        assert!(to_hsl(bright).2 < to_hsl(*normal).2);
    }
}

#[test]
fn red_slot_is_red() {
    let terminal = template_terminal(parse_hex("#8ab8d4").expect("hex"), true);
    let (hue, _, _) = to_hsl(terminal.normal[0]);
    assert!((hue - TEMPLATE_HUES[0]).abs() < 2.0, "{hue}");
}
