#![cfg(test)]

use super::{alloc_count, free_count, peak_bytes};

#[global_allocator]
static GLOBAL: super::Counting<std::alloc::System> = super::system();

#[test]
fn counters_track_box() {
    // These counters are monotonic even when libtest runs sibling tests in parallel.
    // A live-byte delta is not: another test may free memory after this test samples it.
    let allocs = alloc_count();
    let frees = free_count();
    let peak = peak_bytes();
    let block = Box::new([0_u8; 4096]);
    std::hint::black_box(&block);
    assert!(alloc_count() > allocs);
    assert!(peak_bytes() >= peak);
    drop(block);
    assert!(free_count() > frees);
}

#[cfg(feature = "alloc-tap")]
#[test]
fn thread_tap_counts() {
    let before = super::thread_alloc_count();
    let block = vec![0_u8; 64];
    assert!(super::thread_alloc_count() > before);
    drop(block);
    assert!(super::thread_free_count() > 0);
}
