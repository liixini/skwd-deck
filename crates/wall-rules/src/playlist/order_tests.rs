use super::*;

#[test]
fn initial_selection_preserves_sequential_and_small_playlists() {
    let mut rng = 42;
    for order in [Order::Sequential, Order::Shuffle] {
        for len in 0..=1 {
            assert_eq!(initial_index(order, len, &mut rng), 0);
        }
    }
    assert_eq!(initial_index(Order::Sequential, 5, &mut rng), 0);
    assert_eq!(rng, 42);
}

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
