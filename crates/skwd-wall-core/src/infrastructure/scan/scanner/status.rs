use std::sync::atomic::{AtomicBool, Ordering};

static DISK_FULL: AtomicBool = AtomicBool::new(false);

pub fn take_disk_full() -> bool {
    DISK_FULL.swap(false, Ordering::Relaxed)
}

pub(super) fn mark_disk_full() {
    DISK_FULL.store(true, Ordering::Relaxed);
}

pub(super) fn disk_full() -> bool {
    DISK_FULL.load(Ordering::Relaxed)
}
