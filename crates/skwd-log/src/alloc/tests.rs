#![cfg(test)]

use super::{alloc_count, free_count, live_bytes, peak_bytes};

#[global_allocator]
static GLOBAL: super::Counting<std::alloc::System> = super::system();

#[test]
fn counters_track_box() {
    let allocs = alloc_count();
    let live = live_bytes();
    let block = Box::new([0_u8; 4096]);
    assert!(alloc_count() > allocs);
    assert!(live_bytes() >= live + 4096);
    assert!(peak_bytes() >= live_bytes());
    drop(block);
    assert!(free_count() > 0);
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
