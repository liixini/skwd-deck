use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

pub struct ApplyIntent {
    scope: (String, String),
    token: u64,
}

#[derive(Default)]
struct Intents {
    next: u64,
    targets: std::collections::HashMap<(String, String), u64>,
}

#[derive(Default)]
pub struct ApplyRuntime {
    lock: Mutex<()>,
    generation: AtomicU64,
    intents: Mutex<Intents>,
    restore_cancelled: AtomicBool,
    restore_activities: Mutex<std::collections::HashSet<String>>,
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
        self.intents.lock().unwrap_or_else(std::sync::PoisonError::into_inner).targets.clear();
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

    pub fn cancel_restore(&self, activity: Option<&str>) {
        if let Some(activity) = activity {
            self.restore_activities
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(activity.to_owned());
        } else {
            self.restore_cancelled.store(true, Ordering::SeqCst);
        }
    }

    pub fn restore_cancelled(&self, activity: Option<&str>) -> bool {
        self.restore_cancelled.load(Ordering::SeqCst)
            || activity.is_some_and(|activity| {
                self.restore_activities
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(activity)
            })
    }

    pub fn intent(&self, activity: &str, output: &str) -> ApplyIntent {
        let mut intents = self.intents.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        intents.targets.retain(|(scope, target), _| {
            scope != activity
                || (output != wall_proto::apply_fields::ALL_OUTPUTS
                    && target != wall_proto::apply_fields::ALL_OUTPUTS
                    && target != output)
        });
        intents.next += 1;
        let intent =
            ApplyIntent { scope: (activity.to_owned(), output.to_owned()), token: intents.next };
        intents.targets.insert(intent.scope.clone(), intent.token);
        intent
    }

    pub fn intent_current(&self, intent: &ApplyIntent) -> bool {
        self.intents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .targets
            .get(&intent.scope)
            == Some(&intent.token)
    }

    pub fn claim_intent(&self, intent: &ApplyIntent) -> Option<u64> {
        let mut intents = self.intents.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if intents.targets.get(&intent.scope) != Some(&intent.token) {
            return None;
        }
        intents.targets.remove(&intent.scope);
        Some(self.generation.fetch_add(1, Ordering::SeqCst) + 1)
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
