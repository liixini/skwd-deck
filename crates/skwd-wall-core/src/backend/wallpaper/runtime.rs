use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

#[derive(Default)]
pub struct ApplyRuntime {
    lock: Mutex<()>,
    generation: AtomicU64,
    no_transition: AtomicBool,
    swap_slide: Mutex<Option<(String, u64)>>,
    render_fill: Mutex<String>,
    transition_source: Mutex<Option<String>>,
}

impl ApplyRuntime {
    pub fn lock(&self) -> MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn next_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn claim_generation(&self, expected: u64) -> Option<u64> {
        let claimed = expected.saturating_add(1);
        self.generation
            .compare_exchange(expected, claimed, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| claimed)
    }

    pub fn set_no_transition(&self, disabled: bool) {
        self.no_transition.store(disabled, Ordering::Relaxed);
    }

    pub fn no_transition(&self) -> bool {
        self.no_transition.load(Ordering::Relaxed)
    }

    pub fn set_transition_source(&self, source: Option<String>) {
        *self.transition_source.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = source;
    }

    pub fn take_transition_source(&self) -> Option<String> {
        self.transition_source.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take()
    }

    pub fn set_swap_slide(&self, slide: Option<(String, u64)>) {
        *self.swap_slide.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = slide;
    }

    pub fn take_swap_slide(&self) -> Option<(String, u64)> {
        self.swap_slide.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take()
    }

    pub fn render_fill(&self) -> String {
        self.render_fill.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    pub fn set_render_fill(&self, fill: &str) {
        *self.render_fill.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
            fill.to_string();
    }
}
