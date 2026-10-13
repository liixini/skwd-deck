mod activity;
pub use activity::{ACTIVITY_PARAM, ActivitiesResult, Activity, apply_fields};
mod app_themes;
pub use app_themes::{AppThemeRefresh, AppThemeSet, AppThemeStatus, AppThemesResult};
pub mod client;
mod download;
mod envelope;
mod event_catalog;
pub mod helm;
mod library_watch;
mod lockscreen;
pub mod net;
mod output_status;
pub mod picker_control;
mod playlist;
pub mod playlist_source;
mod redaction;
mod removal;
mod renderer;
mod rpc_catalog;
pub mod rpc_params;
mod socket;
pub mod sources;
mod status;
mod task_status;
mod theme;
mod thumbnail;
pub mod transitions;
mod wallpaper_item;
mod we_property;
mod weather;
mod workshop;
mod workspace;
pub use weather::{
    LightPhase, OutdoorMatch, OutdoorOutput, OutdoorSeason, OutdoorStatus, WeatherCandidates,
    WeatherExcludeRequest, WeatherFollowRequest, WeatherOutputRequest, WeatherSelection,
    WeatherSource, WeatherState, WeatherStatus, weather_category, weather_location_key,
};
pub use workshop::workshop_id;

pub use download::{DownloadEvent, dl_status};
pub use envelope::{ErrorInfo, Event, Request, Response, ServerMessage};
pub use event_catalog::ev;
pub use library_watch::{LibraryWatchRootStatus, LibraryWatchStatus, mode as library_watch_mode};
pub use lockscreen::{LockScreenSetRequest, LockScreenSetResult, LockScreenTarget, SddmChoices};
pub use output_status::{OutputStatus, outputs_from};
pub use playlist::{PlaylistAssign, PlaylistRow};
pub const EFFECT_PREVIEW_MAX_EDGE: u32 = 1600;
pub use redaction::{redact_known_secrets, redact_sensitive};
pub use removal::{WallRemoveParams, WallRemoveResult};
pub use renderer::{RendererKind, kind};
pub use rpc_catalog::rpc;
pub use socket::{resolve_socket, socket_path};
pub use status::{
    CAPABILITIES, PROTOCOL_NAME, PROTOCOL_VERSION, SERVICE_COMPONENT, SERVICE_NAME,
    STEAM_CLIENT_AVAILABLE_FIELD, deck_status,
};
pub use task_status::{TaskCapabilities, TaskControl, TaskState, TaskStatus};
pub use theme::THEME_BACKENDS;
pub use thumbnail::{ThumbnailEncodeRequest, ThumbnailEncodeResponse};
pub use wallpaper_item::WallpaperItem;
pub use we_property::{WeProperty, WePropertyOption, we_property_kind};
pub use workspace::WorkspaceRow;

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if passes a field reference"
)]
fn is_false(value: &bool) -> bool {
    !value
}
