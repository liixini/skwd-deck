mod cancellation;
mod decoding;
mod extraction;
mod preview;
mod preview_policy;
mod scaling;
mod source;
mod switching;

#[cfg(test)]
pub(crate) use decoding::is_virtio_vendor;
pub use decoding::stream_video_frames;
#[cfg(test)]
pub(crate) use extraction::{decode_video_frame, mean_plane_value};
pub use extraction::{extract_frame_to, generate_video_thumbs};
pub use preview::generate_video_preview;
pub use preview_policy::{
    PREVIEW_FPS_CAP, PREVIEW_MAX_FRAMES, PREVIEW_QUALITY, PREVIEW_SECONDS, frame_duration_ms,
    keep_preview_frame,
};
pub use switching::stream_video_frames_persist;
