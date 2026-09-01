use super::*;
use crate::seed::chroma_of;

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
fn neutral_images_stay_neutral() {
    let grey =
        image(&[(Rgb(20, 20, 20), 400), (Rgb(128, 128, 128), 400), (Rgb(230, 230, 230), 200)]);
    let selected = seed(&grey, 0, 0, 8).unwrap();
    assert!(chroma_of(selected) < ACHROMATIC_CHROMA);
    for color in [Rgb(0, 0, 0), Rgb(255, 255, 255)] {
        assert!(chroma_of(seed(&image(&[(color, 500)]), 0, 0, 8).unwrap()) < ACHROMATIC_CHROMA);
    }
}

#[test]
fn accents_win_specks_lose() {
    let mixed = image(&[(Rgb(30, 30, 34), 600), (Rgb(200, 60, 40), 400)]);
    let selected = seed(&mixed, 0, 0, 8).unwrap();
    assert!(chroma_of(selected) >= ACHROMATIC_CHROMA);
    assert!(selected.0 > selected.2);

    let speck = image(&[(Rgb(120, 120, 122), 4000), (Rgb(255, 0, 0), 12)]);
    let selected = seed(&speck, 0, 0, 8).unwrap();
    assert!(selected.0.abs_diff(selected.2) < 60);
}

#[test]
fn swatch_order_and_edges() {
    let image = image(&[(Rgb(10, 20, 30), 300), (Rgb(200, 30, 40), 500), (Rgb(90, 200, 90), 200)]);
    let found = swatches(&image, 0, 0, 6);
    let total: f32 = found.iter().map(|swatch| swatch.share).sum();
    assert!((total - 1.0).abs() < 0.01);
    assert!(found.windows(2).all(|pair| pair[0].share >= pair[1].share));
    assert_eq!(found, swatches(&image, 0, 0, 6));

    assert!(seed(&[], 0, 0, 8).is_none());
    assert!(seed(&[1, 2, 3], 0, 0, 8).is_none());
    assert!(swatches(&image, 0, 0, 0).is_empty());
    assert!(pick(&[]).is_none());
    let transparent: Vec<u8> = [255, 0, 0, 0].repeat(50);
    assert!(seed(&transparent, 0, 0, 8).is_none());
}
