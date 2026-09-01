use super::*;

#[test]
fn gate_caps_high_rate() {
    let mut gate = FrameGate::new(30);
    let kept = (0..240).filter(|frame| gate.keep(f64::from(*frame) / 120.0)).count();
    assert!((59..=61).contains(&kept), "kept {kept}");
}

#[test]
fn gate_fractional_ratio() {
    let mut gate = FrameGate::new(20);
    let kept = (0..300).filter(|frame| gate.keep(f64::from(*frame) / 30.0)).count();
    assert!((199..=201).contains(&kept), "kept {kept}");
}

#[test]
fn gate_below_cap() {
    let mut gate = FrameGate::new(30);
    assert_eq!((0..48).filter(|frame| gate.keep(f64::from(*frame) / 24.0)).count(), 48);
}

#[test]
fn gate_timestamp_jump() {
    let mut gate = FrameGate::new(30);
    let jump = 1_000_000.0;

    assert!(gate.keep(0.0));
    assert!(gate.keep(jump));
    let next = gate.next.expect("next frame deadline");
    assert!(next > jump);
    assert!(next <= jump + gate.interval + 1e-6);
}

#[test]
fn gate_rejects_nonfinite() {
    let mut gate = FrameGate::new(30);

    assert!(!gate.keep(f64::NAN));
    assert!(!gate.keep(f64::INFINITY));
    assert!(gate.keep(0.0));
}
