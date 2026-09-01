use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static FREES: AtomicUsize = AtomicUsize::new(0);

#[cfg(feature = "alloc-tap")]
thread_local! {
    static THREAD_ALLOCS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static THREAD_FREES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub struct Counting<A>(pub A);

pub const fn system() -> Counting<System> {
    Counting(System)
}

fn record_alloc(size: usize) {
    ALLOCS.fetch_add(1, Relaxed);
    #[cfg(feature = "alloc-tap")]
    THREAD_ALLOCS.with(|count| count.set(count.get() + 1));
    let live = LIVE.fetch_add(size, Relaxed) + size;
    PEAK.fetch_max(live, Relaxed);
}

fn record_grow(delta: usize) {
    let live = LIVE.fetch_add(delta, Relaxed) + delta;
    PEAK.fetch_max(live, Relaxed);
}

fn record_free(size: usize) {
    FREES.fetch_add(1, Relaxed);
    #[cfg(feature = "alloc-tap")]
    THREAD_FREES.with(|count| count.set(count.get() + 1));
    LIVE.fetch_sub(size, Relaxed);
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for Counting<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { self.0.alloc(layout) };
        if !ptr.is_null() {
            record_alloc(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.0.dealloc(ptr, layout) };
        record_free(layout.size());
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { self.0.alloc_zeroed(layout) };
        if !ptr.is_null() {
            record_alloc(layout.size());
        }
        ptr
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { self.0.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            let old = layout.size();
            if new_size >= old {
                record_grow(new_size - old);
            } else {
                LIVE.fetch_sub(old - new_size, Relaxed);
            }
        }
        new_ptr
    }
}

#[must_use]
pub fn live_bytes() -> usize {
    LIVE.load(Relaxed)
}

#[must_use]
pub fn peak_bytes() -> usize {
    PEAK.load(Relaxed)
}

#[must_use]
pub fn alloc_count() -> usize {
    ALLOCS.load(Relaxed)
}

#[must_use]
pub fn free_count() -> usize {
    FREES.load(Relaxed)
}

#[cfg(feature = "alloc-tap")]
#[must_use]
pub fn thread_alloc_count() -> usize {
    THREAD_ALLOCS.get()
}

#[cfg(feature = "alloc-tap")]
#[must_use]
pub fn thread_free_count() -> usize {
    THREAD_FREES.get()
}

mod tests;
