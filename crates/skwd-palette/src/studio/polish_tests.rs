use super::*;

#[test]
fn ensure_reaches_target() {
    let base = Rgb(30, 30, 34);
    let lifted = ensure(Rgb(60, 60, 64), base, TEXT_CONTRAST);
    assert!(contrast(lifted, base) >= TEXT_CONTRAST);
    let kept = Rgb(240, 240, 240);
    assert_eq!(ensure(kept, base, TEXT_CONTRAST), kept);
}

#[test]
fn fit_moves_away() {
    let base = Rgb(250, 248, 245);
    let fitted = fit(Oklch { light: 0.8, chroma: 0.1, hue: 30.0 }, base, READABLE_CONTRAST, false);
    assert!(contrast(fitted, base) >= READABLE_CONTRAST);
}

#[test]
fn pulls_toward_wallpaper() {
    assert_eq!(anchored(29.0, &[], WALLPAPER_TERMINAL.pull), 29.0);
    let pulled = anchored(29.0, &[Pull { hue: 45.0, strength: 1.0 }], WALLPAPER_TERMINAL.pull);
    assert!(pulled > 29.0 && pulled <= 29.0 + anchor_cap(29.0) + 1e-3, "{pulled}");
    assert_eq!(
        anchored(29.0, &[Pull { hue: 120.0, strength: 1.0 }], WALLPAPER_TERMINAL.pull),
        29.0
    );
}
