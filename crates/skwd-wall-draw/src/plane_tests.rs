use super::*;

#[test]
fn percentile_interpolates() {
    let plane = Plane { width: 5, height: 1, data: vec![4.0, 0.0, 2.0, 1.0, 3.0] };
    assert_eq!(plane.percentile(0.0), 0.0);
    assert_eq!(plane.percentile(50.0), 2.0);
    assert_eq!(plane.percentile(100.0), 4.0);
    assert_eq!(plane.percentile(62.5), 2.5);
}

#[test]
fn sample_clamps_edges() {
    let plane = Plane { width: 2, height: 2, data: vec![0.0, 1.0, 2.0, 3.0] };
    assert_eq!(plane.sample([-4.0, 9.0]), 2.0);
    assert_eq!(plane.sample([0.6, 0.4]), 1.0);
}

#[test]
fn resize_keeps_range() {
    let plane = Plane { width: 2, height: 1, data: vec![-6.0, 14.0] };
    let resized = plane.resize(4, 1, FilterType::Triangle);
    assert_eq!(resized.data.first().copied(), Some(-6.0));
    assert_eq!(resized.data.last().copied(), Some(14.0));
    assert!(resized.data.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[test]
fn resize_keeps_constant() {
    let plane = Plane::filled(3, 2, 4.5);
    assert!(plane.resize(6, 4, FilterType::Triangle).data.iter().all(|&value| value == 4.5));
}

#[test]
fn bytes_round_and_clamp() {
    let plane = Plane { width: 3, height: 1, data: vec![-1.0, 0.5, 2.0] };
    assert_eq!(plane.to_bytes(), vec![0, 128, 255]);
}

#[test]
fn crop_takes_window() {
    let plane = Plane { width: 3, height: 2, data: vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0] };
    let cropped = plane.crop(1, 1, 2, 1);
    assert_eq!((cropped.width, cropped.height, cropped.data), (2, 1, vec![4.0, 5.0]));
}

#[test]
fn mean_clamps_window() {
    let plane = Plane { width: 3, height: 1, data: vec![0.0, 3.0, 6.0] };
    assert_eq!(plane.mean(1).data, vec![1.5, 3.0, 4.5]);
}

#[test]
fn guided_keeps_flat() {
    let guide = Plane { width: 4, height: 1, data: vec![0.1, 0.9, 0.2, 0.7] };
    let flat = Plane::filled(4, 1, 0.6).guided(&guide, 1, 1e-4);
    assert!(flat.data.iter().all(|value| (value - 0.6).abs() < 1e-5));
}

#[test]
fn guided_follows_edge() {
    let guide = Plane { width: 8, height: 1, data: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0] };
    let blurry = Plane { width: 8, height: 1, data: vec![0.0, 0.0, 0.2, 0.4, 0.6, 0.8, 1.0, 1.0] };
    let refined = blurry.guided(&guide, 2, 1e-4);
    assert!(refined.data[3] < blurry.data[3]);
    assert!(refined.data[4] > blurry.data[4]);
}

#[test]
fn above_is_binary() {
    let plane = Plane { width: 3, height: 1, data: vec![0.2, 0.5, 0.7] };
    assert_eq!(plane.above(0.5).data, vec![0.0, 0.0, 1.0]);
}
