use super::*;

fn block_image(blocks: &[Rgb]) -> Vec<u8> {
    let mut image = Vec::new();
    for color in blocks {
        for _ in 0..256 {
            image.extend_from_slice(&[color.0, color.1, color.2, 255]);
        }
    }
    image
}

#[test]
fn distinct_colors_survive_quantization() {
    let image = block_image(&[Rgb(220, 20, 20), Rgb(20, 20, 220)]);
    let colors = quantize(&image, 2);
    assert_eq!(colors.len(), 2);
    assert!(colors.iter().any(|color| color.0 > 150 && color.2 < 80));
    assert!(colors.iter().any(|color| color.2 > 150 && color.0 < 80));
}

#[test]
fn degenerate_images_are_safe() {
    assert!(quantize(&[], 4).is_empty());
    assert!(quantize(&[10, 20, 30, 0, 40, 50, 60, 0], 4).is_empty());
    let solid = block_image(&[Rgb(120, 60, 200)]);
    assert_eq!(quantize(&solid, 8), vec![Rgb(120, 60, 200)]);
}

#[test]
fn saturated_beats_grey() {
    let mut blocks = vec![Rgb(128, 128, 128); 7];
    blocks.push(Rgb(220, 40, 40));
    let colors = quantize(&block_image(&blocks), 4);
    assert!(colors.len() >= 2);
    assert!(colors[0].0 > 150 && colors[0].1 < 100);
}
