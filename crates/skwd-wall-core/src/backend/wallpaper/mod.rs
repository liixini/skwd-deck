mod application;
mod runtime;

pub use application::{
    ApplyFanout, ApplyOutputRequest, ApplyStaticRequest, ApplyVideoRequest,
    OutputTransitionRequest, StaticSmartRequest, VideoTransitionRequest, WallpaperApplication,
};
pub use runtime::{ApplyIntent, ApplyPhase, ApplyPhases, ApplyRuntime, PhaseRecording};

#[cfg(test)]
mod tests;
