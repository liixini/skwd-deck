mod adapter;
mod client;
mod composition;
mod stationary;
pub(crate) use stationary::configure_stationary;
pub use stationary::{set_stationary_overview, stationary_surface, stationary_surfaces};

pub use adapter::PaperClientAdapter;
pub(crate) use composition::sharing;

pub use client::{PaperClient, Startup, paper_socket_path, shares_workers, socket_at};
pub use composition::{
    PaperCompositionPlan, PaperCompositionResult, renderer_policy, stationary_wallpaper,
};
pub use paper_control::{
    ApplyRequest, ApplyResult, Assignment, AssignmentStatus, CapabilitiesResult,
    ControlCapabilities, FillMode, Layer, PROTOCOL_NAME, PROTOCOL_VERSION, PauseRequest,
    RendererCapability, RendererDiscovery, RendererPolicy, RendererPolicyCapabilities,
    RuntimeDependencyStatus, SandPolicy, SandQuality, SandScope, ScenePolicy, Source, SourceKind,
    StatusResult, StopResult, SurfacePolicy, TransitionCapabilities, TransitionPolicy,
    decode_ndjson, encode_ndjson,
};

#[cfg(test)]
mod tests;
