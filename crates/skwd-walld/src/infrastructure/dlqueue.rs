use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, PoisonError};

pub static PREVIEWS: Gate = Gate::with_queue(2, 16);
pub static IMAGES: Gate = Gate::with_queue(3, 12);
pub static VIDEOS: Gate = Gate::with_queue(2, 6);

struct Line {
    issued: u64,
    admitted: u64,
    active: usize,
}

pub struct Gate {
    cap: usize,
    max_outstanding: usize,
    outstanding: AtomicUsize,
    line: Mutex<Line>,
    cv: Condvar,
}

pub struct Reservation<'a> {
    gate: &'a Gate,
    held: bool,
}

pub struct Slot<'a> {
    gate: &'a Gate,
}

impl Gate {
    #[cfg(test)]
    pub const fn new(cap: usize) -> Self {
        Self::with_queue(cap, cap * 4)
    }

    pub const fn with_queue(cap: usize, max_queued: usize) -> Self {
        Self {
            cap,
            max_outstanding: cap + max_queued,
            outstanding: AtomicUsize::new(0),
            line: Mutex::new(Line { issued: 0, admitted: 0, active: 0 }),
            cv: Condvar::new(),
        }
    }

    pub fn try_reserve(&self) -> Option<Reservation<'_>> {
        let mut current = self.outstanding.load(Ordering::Acquire);
        loop {
            if current >= self.max_outstanding {
                return None;
            }
            match self.outstanding.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(Reservation { gate: self, held: true }),
                Err(observed) => current = observed,
            }
        }
    }

    fn acquire_reserved(&self, mut on_wait: impl FnMut(u64)) -> Slot<'_> {
        let mut line = self.line.lock().unwrap_or_else(PoisonError::into_inner);
        let my = line.issued;
        line.issued += 1;
        let mut told = u64::MAX;
        loop {
            if my == line.admitted && line.active < self.cap {
                line.admitted += 1;
                line.active += 1;
                drop(line);
                self.cv.notify_all();
                return Slot { gate: self };
            }
            let ahead = my - line.admitted;
            if ahead != told {
                told = ahead;
                on_wait(ahead);
            }
            line = self.cv.wait(line).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

impl<'a> Reservation<'a> {
    pub fn acquire(mut self, on_wait: impl FnMut(u64)) -> Slot<'a> {
        let slot = self.gate.acquire_reserved(on_wait);
        self.held = false;
        slot
    }
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if self.held {
            self.gate.outstanding.fetch_sub(1, Ordering::AcqRel);
            self.gate.cv.notify_all();
        }
    }
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        let mut line = self.gate.line.lock().unwrap_or_else(PoisonError::into_inner);
        line.active = line.active.saturating_sub(1);
        drop(line);
        self.gate.outstanding.fetch_sub(1, Ordering::AcqRel);
        self.gate.cv.notify_all();
    }
}

pub fn queue_label(ahead: u64) -> String {
    if ahead == 0 { "queued".into() } else { format!("queued - {ahead} ahead") }
}

mod tests;
