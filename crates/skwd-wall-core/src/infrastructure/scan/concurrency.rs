use std::path::Path;

use image::ImageDecoder;

use crate::lock;

pub(super) const DECODE_MEMORY_DEFAULT_MIB: usize = 256;
pub(super) const DECODE_MEMORY_MIN_MIB: usize = 64;
pub(super) const DECODE_MAX_CONCURRENT: usize = 3;

const DECODE_FIXED_OVERHEAD_BYTES: usize = 32 * 1024 * 1024;

pub(super) fn scan_threads() -> usize {
    if let Some(thread_count) =
        std::env::var("SKWD_SCAN_THREADS").ok().and_then(|value| value.parse::<usize>().ok())
    {
        return thread_count.clamp(1, 32);
    }
    std::thread::available_parallelism().map_or(2, |parallelism| parallelism.get().clamp(1, 8))
}

pub(super) fn decode_max_concurrent() -> usize {
    if let Some(permit_count) = std::env::var("SKWD_IMAGE_DECODE_PERMITS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
    {
        return permit_count.clamp(1, 32);
    }
    scan_threads().clamp(1, DECODE_MAX_CONCURRENT)
}

pub(super) fn decode_memory_budget_bytes() -> usize {
    std::env::var("SKWD_SCAN_MEMORY_MIB")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DECODE_MEMORY_DEFAULT_MIB)
        .clamp(DECODE_MEMORY_MIN_MIB, 2048)
        .saturating_mul(1024 * 1024)
}

pub(super) fn image_decode_weight(path: &Path) -> usize {
    let pixel_floor = image::image_dimensions(path)
        .ok()
        .and_then(|(width, height)| {
            usize::try_from(u64::from(width).saturating_mul(u64::from(height)).saturating_mul(4))
                .ok()
        })
        .unwrap_or(DECODE_FIXED_OVERHEAD_BYTES);
    let decoded = image::ImageReader::open(path)
        .and_then(image::ImageReader::with_guessed_format)
        .ok()
        .and_then(|reader| reader.into_decoder().ok())
        .and_then(|decoder| usize::try_from(decoder.total_bytes()).ok())
        .map_or(pixel_floor, |bytes| bytes.max(pixel_floor));
    decoded.saturating_add(DECODE_FIXED_OVERHEAD_BYTES)
}

#[derive(Debug)]
struct BudgetState {
    available_bytes: usize,
    active_jobs: usize,
}

pub(super) struct DecodeBudget {
    capacity_bytes: usize,
    max_jobs: usize,
    state: std::sync::Mutex<BudgetState>,
    condition: std::sync::Condvar,
}

impl DecodeBudget {
    pub(super) fn new(capacity_bytes: usize, max_jobs: usize) -> Self {
        let capacity_bytes = capacity_bytes.max(1);
        Self {
            capacity_bytes,
            max_jobs: max_jobs.max(1),
            state: std::sync::Mutex::new(BudgetState {
                available_bytes: capacity_bytes,
                active_jobs: 0,
            }),
            condition: std::sync::Condvar::new(),
        }
    }

    pub(super) fn acquire(&self, requested_bytes: usize) -> DecodeBudgetGuard<'_> {
        // Clamping admits an oversize job alone instead of deadlocking on it forever.
        let charged_bytes = requested_bytes.clamp(1, self.capacity_bytes);
        let mut state = lock(&self.state);
        while state.available_bytes < charged_bytes || state.active_jobs >= self.max_jobs {
            state = self.condition.wait(state).unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        state.available_bytes -= charged_bytes;
        state.active_jobs += 1;
        DecodeBudgetGuard { budget: self, charged_bytes }
    }

    pub(super) fn acquire_exclusive(&self) -> DecodeBudgetGuard<'_> {
        self.acquire(self.capacity_bytes)
    }
}

pub(super) struct DecodeBudgetGuard<'a> {
    budget: &'a DecodeBudget,
    charged_bytes: usize,
}

impl Drop for DecodeBudgetGuard<'_> {
    fn drop(&mut self) {
        let mut state = lock(&self.budget.state);
        state.available_bytes = state
            .available_bytes
            .saturating_add(self.charged_bytes)
            .min(self.budget.capacity_bytes);
        state.active_jobs = state.active_jobs.saturating_sub(1);
        self.budget.condition.notify_all();
    }
}

static DECODE_BUDGET: std::sync::OnceLock<DecodeBudget> = std::sync::OnceLock::new();

pub(super) fn decode_budget() -> &'static DecodeBudget {
    DECODE_BUDGET
        .get_or_init(|| DecodeBudget::new(decode_memory_budget_bytes(), decode_max_concurrent()))
}
