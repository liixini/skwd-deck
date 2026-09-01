mod application;
mod runtime;

pub use application::{
    ApplyOutputRequest, ApplyStaticRequest, ApplyVideoRequest, OutputTransitionRequest,
    StaticSmartRequest, VideoTransitionRequest, WallpaperApplication,
};
pub use runtime::ApplyRuntime;

#[cfg(test)]
mod tests;
