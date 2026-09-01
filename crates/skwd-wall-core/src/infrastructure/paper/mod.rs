mod adapter;
mod client;
mod composition;

pub use adapter::PaperClientAdapter;
pub use client::{PaperClient, paper_socket_path, socket_at};
pub(crate) use composition::tinier_or_default_source;
pub use composition::{PaperCompositionPlan, PaperCompositionResult, renderer_policy};
pub use paper_control::{
    ApplyRequest, ApplyResult, Assignment, AssignmentStatus, CapabilitiesResult,
    ControlCapabilities, FillMode, Layer, PROTOCOL_NAME, PROTOCOL_VERSION, RendererCapability,
    RendererDiscovery, RendererPolicy, RendererPolicyCapabilities, RuntimeDependencyStatus,
    SandPolicy, SandQuality, SandScope, ScenePolicy, Source, SourceKind, StatusResult, StopResult,
    TransitionCapabilities, TransitionPolicy, VideoEngine, decode_ndjson, encode_ndjson,
};

#[cfg(test)]
mod tests;
