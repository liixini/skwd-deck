use super::*;

#[test]
fn sequential_step_wraps() {
    let mut rng = 1;
    assert_eq!(step(Order::Sequential, 0, 3, true, &mut rng), 1);
    assert_eq!(step(Order::Sequential, 2, 3, true, &mut rng), 0);
    assert_eq!(step(Order::Sequential, 0, 3, false, &mut rng), 2);
    assert_eq!(step(Order::Sequential, 0, 1, true, &mut rng), 0);
}

#[test]
fn shuffle_skips_cursor() {
    let mut rng = 0x1234_5678;
    for _ in 0..50 {
        let next = step(Order::Shuffle, 2, 5, true, &mut rng);
        assert!(next < 5 && next != 2, "got {next}");
    }
}
