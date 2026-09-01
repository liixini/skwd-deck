pub const PREVIEW_QUALITY: f32 = 70.0;
pub const PREVIEW_SECONDS: f64 = 3.0;
pub const PREVIEW_MAX_FRAMES: usize = 180;
pub const PREVIEW_FPS_CAP: f64 = 20.0;

pub(super) struct FrameGate {
    interval: f64,
    next: Option<f64>,
    last: Option<f64>,
}

impl FrameGate {
    pub(super) fn new(fps: u32) -> Self {
        Self { interval: 1.0 / f64::from(fps.clamp(1, 60)), next: None, last: None }
    }

    pub(super) fn keep(&mut self, seconds: f64) -> bool {
        if !seconds.is_finite() {
            return false;
        }
        if self.last.is_some_and(|last| seconds + 1e-6 < last) {
            self.next = None;
        }
        self.last = Some(seconds);
        let Some(mut next) = self.next else {
            self.next = Some(seconds + self.interval);
            return true;
        };
        if seconds + 1e-6 < next {
            return false;
        }
        let intervals = ((seconds + 1e-6 - next) / self.interval).floor() + 1.0;
        next += intervals * self.interval;
        self.next = Some(next);
        true
    }
}

pub fn keep_preview_frame(last_kept: f64, frame_seconds: f64, fps_cap: f64) -> bool {
    frame_seconds - last_kept >= 1.0 / fps_cap - 1e-6
}

pub fn frame_duration_ms(timestamps: &[f64], index: usize) -> i32 {
    let frame_count = timestamps.len();
    let duration = if index + 1 < frame_count {
        timestamps[index + 1] - timestamps[index]
    } else if index > 0 {
        timestamps[index] - timestamps[index - 1]
    } else {
        1.0 / 24.0
    };
    ((duration * 1000.0).round() as i32).clamp(10, 200)
}

#[cfg(test)]
mod tests;
