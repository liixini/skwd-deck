use super::*;
use crate::Rgb;

fn image(colors: &[(Rgb, usize)]) -> Vec<u8> {
    let mut output = Vec::new();
    for (color, count) in colors {
        for _ in 0..*count {
            output.extend_from_slice(&[color.0, color.1, color.2, 255]);
        }
    }
    output
}

#[test]
fn auto_follows_brightness() {
    let dark = image(&[(Rgb(20, 24, 30), 800), (Rgb(60, 70, 80), 200)]);
    assert!(tone(&dark).prefers_dark());

    let light = image(&[(Rgb(232, 236, 240), 800), (Rgb(180, 190, 200), 200)]);
    assert!(!tone(&light).prefers_dark());
}

#[test]
fn auto_uses_average() {
    let split =
        image(&[(Rgb(10, 10, 10), 300), (Rgb(128, 128, 128), 400), (Rgb(245, 245, 245), 300)]);
    let tone = tone(&split);
    assert!(tone.dark_ratio <= 0.52 && tone.light_ratio <= 0.52);
    assert_eq!(tone.prefers_dark(), tone.avg_light < 0.50);
}
