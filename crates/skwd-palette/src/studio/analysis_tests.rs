use super::*;

fn image(width: usize, height: usize, paint: impl Fn(usize, usize) -> [u8; 3]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(width * height * RGBA);
    for row in 0..height {
        for col in 0..width {
            let [red, green, blue] = paint(col, row);
            rgba.extend_from_slice(&[red, green, blue, u8::MAX]);
        }
    }
    rgba
}

#[test]
fn finds_shares() {
    let rgba = image(20, 10, |col, _| if col < 15 { [20, 20, 30] } else { [220, 60, 50] });
    let analysis = Analysis::new(&rgba, 20, 10).expect("analysis");
    let red = analysis.seed();
    assert!((red.share - 0.25).abs() < 0.01, "{}", red.share);
    assert!(red.x > 0.8, "{}", red.x);
    assert!(!analysis.grey);
}

#[test]
fn grey_detected() {
    let rgba = image(10, 10, |col, _| if col < 5 { [30, 30, 30] } else { [200, 200, 200] });
    assert!(Analysis::new(&rgba, 10, 10).expect("analysis").grey);
}

#[test]
fn crops_area() {
    let rgba = image(10, 10, |col, row| [col as u8, row as u8, 0]);
    let area = Area { left: 0.5, top: 0.2, right: 1.0, bottom: 0.4 };
    let (cropped, width, height) = crop(&rgba, 10, 10, area);
    assert_eq!((width, height), (5, 2));
    assert_eq!(&cropped[..3], &[5, 2, 0]);
}

#[test]
fn others_keep_gap() {
    let rgba = image(30, 10, |col, _| match col / 10 {
        0 => [220, 60, 50],
        1 => [230, 90, 50],
        _ => [50, 120, 220],
    });
    let analysis = Analysis::new(&rgba, 30, 10).expect("analysis");
    let seed = analysis.seed();
    let others = analysis.others(seed.lch.hue, 2, 40.0);
    assert_eq!(others.len(), 1);
    assert!(others[0].lch.hue > 200.0);
}
