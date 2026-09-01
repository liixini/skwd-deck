use std::time::Duration;

use super::{content_progress, should_emit, stream_to};

#[test]
fn content_progress_bounds() {
    assert_eq!(content_progress(0, 1000), Some(0.0));
    assert_eq!(content_progress(500, 1000), Some(0.5));
    assert_eq!(content_progress(1000, 1000), Some(0.99));
    assert_eq!(content_progress(2000, 1000), Some(0.99));
    assert_eq!(content_progress(500, 0), None);
}

#[test]
fn stream_to_exact() {
    let dir = crate::testenv::tmp("streamto");
    let path = dir.join("out.bin");
    let data: Vec<u8> = (0..300_000u32).map(|idx| (idx % 251) as u8).collect();
    let mut seen: Vec<f64> = Vec::new();
    {
        let mut file = std::fs::File::create(&path).unwrap();
        let mut source = std::io::Cursor::new(data.clone());
        stream_to(&mut source, &mut file, data.len() as u64, data.len() as u64, &mut |percent| {
            seen.push(percent);
        })
        .unwrap();
    }
    assert_eq!(std::fs::read(&path).unwrap(), data);
    assert!(seen.iter().all(|percent| (0.0..=0.99).contains(percent)));
    assert!(seen.windows(2).all(|window| window[1] > window[0]));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stream_to_over_limit() {
    let dir = crate::testenv::tmp("streamto-overflow");
    let path = dir.join("out.bin");
    let mut file = std::fs::File::create(&path).unwrap();
    let mut source = std::io::Cursor::new(vec![7_u8; 17]);
    let error = stream_to(&mut source, &mut file, 0, 16, &mut |_| {}).unwrap_err();
    assert!(error.to_string().contains("exceeds 16 byte limit"));
    drop(file);
    assert!(std::fs::metadata(&path).unwrap().len() <= 16);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn emit_throttle() {
    let slow = Duration::from_millis(200);
    assert!(should_emit(0.05, 0.0, slow));
    assert!(!should_emit(0.005, 0.0, slow));
    assert!(!should_emit(0.5, 0.0, Duration::from_millis(20)));
    assert!(!should_emit(0.5, 0.5, slow));
}
