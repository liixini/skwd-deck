use super::*;
use crate::Rgb;
use crate::studio::palette::ANSI_SLOTS;

fn sample() -> StudioPalette {
    StudioPalette {
        base: Rgb(20, 22, 30),
        panel: Rgb(30, 32, 40),
        raised: Rgb(42, 44, 52),
        text: Rgb(235, 236, 240),
        quiet: Rgb(150, 152, 160),
        accent: Rgb(240, 100, 90),
        on_accent: Rgb(30, 10, 10),
        second: Rgb(120, 180, 210),
        third: Rgb(200, 180, 120),
        ansi: [Rgb(200, 90, 80); ANSI_SLOTS],
    }
}

#[test]
fn default_is_identity() {
    let palette = sample();
    assert_eq!(tune(&palette, &Tuning::default(), true), palette);
}

#[test]
fn warmth_moves_hue() {
    let warm = tune(&sample(), &Tuning { warmth: 1.0, ..Tuning::default() }, true);
    let cool = tune(&sample(), &Tuning { warmth: -1.0, ..Tuning::default() }, true);
    assert!(warm.base.0 > cool.base.0 && warm.base.2 < cool.base.2);
}

#[test]
fn colour_scales_accents() {
    let soft = tune(&sample(), &Tuning { colour: 0.4, ..Tuning::default() }, true);
    assert!(to_oklch(soft.accent).chroma < to_oklch(sample().accent).chroma);
    assert!(to_oklch(soft.ansi[1]).chroma < to_oklch(sample().ansi[1]).chroma);
    assert_eq!(soft.ansi[0], sample().ansi[0]);
}

#[test]
fn contrast_spreads_neutrals() {
    let high = tune(&sample(), &Tuning { contrast: 1.0, ..Tuning::default() }, true);
    assert!(to_oklch(high.base).light < to_oklch(sample().base).light);
    assert!(to_oklch(high.text).light > to_oklch(sample().text).light);
}

#[test]
fn shade_steps_role() {
    let mut tuning = Tuning::default();
    tuning.shades[Role::Accent.index()] = 2;
    let lifted = tune(&sample(), &tuning, true);
    let gap = to_oklch(lifted.accent).light - to_oklch(sample().accent).light;
    assert!((gap - 2.0 * SHADE_STEP).abs() < 0.01, "{gap}");
    assert_eq!(lifted.panel, sample().panel);
}

#[test]
fn shaded_accent_readable() {
    let palette = crate::studio::from_theme(&crate::preset("nord").expect("nord"));
    let mut tuning = Tuning::default();
    tuning.shades[Role::Accent.index()] = SHADE_RANGE.0;
    let tuned = tune(&palette, &tuning, true);
    assert!(crate::semantic::contrast(tuned.on_accent, tuned.accent) >= READABLE_CONTRAST);
}

#[test]
fn parts_clamp_shades() {
    let tuning = Tuning::from_parts(0.5, 1.2, -0.25, [9, -9, 2]);
    assert_eq!(&tuning.shades[..3], &[SHADE_RANGE.1, SHADE_RANGE.0, 2]);
    assert_eq!(tuning.shades[3], 0);
    assert_eq!(tuning.warmth, 0.5);
}

#[test]
fn ensure_mid_grey() {
    let grey = crate::studio::oklch(0.59, 0.0, 0.0);
    let ink = super::super::polish::ensure(grey, grey, READABLE_CONTRAST);
    assert_eq!(ink, Rgb(0, 0, 0));
}
