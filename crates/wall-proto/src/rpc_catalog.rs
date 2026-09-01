pub mod rpc {
    pub const STATUS: &str = "status";
    pub const STATUS_DOCTOR: &str = "status.doctor";
    pub const STATUS_BUG_REPORT: &str = "status.bug_report";
    pub const SUBSCRIBE: &str = "subscribe";
    pub const DIAG: &str = "diag";
    pub const PAPER_READY: &str = "paper.ready";

    pub const PICKER_SESSION_BEGIN: &str = "picker.session.begin";
    pub const PICKER_SESSION_END: &str = "picker.session.end";

    pub const WALL_LIST: &str = "wall.list";
    pub const WALL_APPLY: &str = "wall.apply";
    pub const WALL_PREHEAT: &str = "wall.preheat";
    pub const WALL_REMOVE: &str = "wall.remove";
    pub const WALL_OUTPUTS: &str = "wall.outputs";
    pub const WALL_WEATHER: &str = "wall.weather";
    pub const WALL_RETHEME: &str = "wall.retheme";
    pub const WALL_RELOAD_WE: &str = "wall.reload_we";
    pub const WALL_WE_PROPERTIES: &str = "wall.we_properties";
    pub const WALL_SET_WE_PROPERTY: &str = "wall.set_we_property";
    pub const WALL_CLEAR_DATA: &str = "wall.clear_data";
    pub const WALL_SET_PAUSED: &str = "wall.set_paused";
    pub const WALL_SET_AUDIO: &str = "wall.set_audio";
    pub const WALL_SET_FAVOURITE: &str = "wall.set_favourite";
    pub const WALL_ROTATION_WAKE: &str = "wall.rotation_wake";
    pub const WALL_MONITORS: &str = "wall.monitors";
    pub const WALL_FORGET_MONITOR: &str = "wall.forget_monitor";
    pub const WALL_SHELL_PREVIEW: &str = "wall.shell_preview";
    pub const WALL_SHELL_PREVIEW_END: &str = "wall.shell_preview_end";
    pub const WALL_RECOMPUTE_COLORS: &str = "wall.recompute_colors";
    pub const WALL_REFRESH_OVERVIEW_BACKDROP: &str = "wall.refresh_overview_backdrop";
    pub const WALL_UPDATE_TAGS: &str = "wall.update_tags";
    pub const WALL_UPDATE_ANALYSIS: &str = "wall.update_analysis";
    pub const WALL_HISTORY_BACK: &str = "wall.history.back";
    pub const WALL_HISTORY_FORWARD: &str = "wall.history.forward";
    pub const WALL_HISTORY_LIST: &str = "wall.history.list";
    pub const WALL_PLAYLIST_NEXT: &str = "wall.playlist.next";
    pub const WALL_PLAYLIST_PREV: &str = "wall.playlist.prev";
    pub const WALL_PLAYLIST_RELOAD: &str = "wall.playlist.reload";

    pub const PLAYLIST_LIST: &str = "playlist.list";
    pub const PLAYLIST_CREATE: &str = "playlist.create";
    pub const PLAYLIST_DELETE: &str = "playlist.delete";
    pub const PLAYLIST_UPDATE: &str = "playlist.update";
    pub const PLAYLIST_MEMBERS: &str = "playlist.members";
    pub const PLAYLIST_MEMBERSHIPS: &str = "playlist.memberships";
    pub const PLAYLIST_ADD: &str = "playlist.add";
    pub const PLAYLIST_REMOVE: &str = "playlist.remove";
    pub const PLAYLIST_TOGGLE: &str = "playlist.toggle";
    pub const PLAYLIST_MOVE: &str = "playlist.move";
    pub const PLAYLIST_ASSIGN: &str = "playlist.assign";
    pub const PLAYLIST_STOP: &str = "playlist.stop";

    pub const SCHEDULE_RELOAD: &str = "schedule.reload";
    pub const WORKSPACE_RELOAD: &str = "workspace.reload";
    pub const WORKSPACE_LIST: &str = "workspace.list";

    pub const SCAN_ITEM: &str = "scan.item";
    pub const SCAN_DONE: &str = "scan.done";
    pub const SCAN_REMOVED: &str = "scan.removed";
    pub const REMOTE_THUMB: &str = "remote.thumb";
    pub const PREVIEW_READY: &str = "preview.ready";
    pub const RECOMPUTE_PROGRESS: &str = "recompute.progress";
    pub const RECOMPUTE_DONE: &str = "recompute.done";

    pub const EFFECTS_LIST: &str = "effects.list";
    pub const EFFECTS_PREVIEW: &str = "effects.preview";
    pub const EFFECTS_COMMIT: &str = "effects.commit";
    pub const EFFECTS_DISCARD: &str = "effects.discard";
    pub const EFFECTS_BACKFILL_TAGS: &str = "effects.backfill_tags";

    pub const THEME_BACKENDS: &str = "theme.backends";
    pub const THEME_PREVIEW: &str = "theme.preview";
    pub const THEME_PREVIEWS: &str = "theme.previews";

    pub const TASK_LIST: &str = "task.list";
    pub const TASK_CONTROL: &str = "task.control";

    pub const OPTIMIZE_START: &str = "optimize.start";
    pub const OPTIMIZE_STATUS: &str = "optimize.status";

    pub const SOURCE_LIST: &str = "source.list";
    pub const SOURCE_PREVIEW: &str = "source.preview";
    pub const SOURCE_DOWNLOAD: &str = "source.download";
    pub const WALLHAVEN_SEARCH: &str = "wallhaven.search";
    pub const WALLHAVEN_COLLECTIONS: &str = "wallhaven.collections";
    pub const WALLHAVEN_PREVIEW: &str = "wallhaven.preview";
    pub const WALLHAVEN_DOWNLOAD: &str = "wallhaven.download";
    pub const STEAM_SEARCH: &str = "steam.search";
    pub const STEAM_PREVIEW: &str = "steam.preview";
    pub const STEAM_DOWNLOAD: &str = "steam.download";

    pub const ALL: &[&str] = &[
        STATUS,
        STATUS_DOCTOR,
        STATUS_BUG_REPORT,
        SUBSCRIBE,
        DIAG,
        PAPER_READY,
        PICKER_SESSION_BEGIN,
        PICKER_SESSION_END,
        WALL_LIST,
        WALL_APPLY,
        WALL_PREHEAT,
        WALL_REMOVE,
        WALL_OUTPUTS,
        WALL_WEATHER,
        WALL_RETHEME,
        WALL_RELOAD_WE,
        WALL_WE_PROPERTIES,
        WALL_SET_WE_PROPERTY,
        WALL_CLEAR_DATA,
        WALL_SET_PAUSED,
        WALL_SET_AUDIO,
        WALL_SET_FAVOURITE,
        WALL_ROTATION_WAKE,
        WALL_MONITORS,
        WALL_FORGET_MONITOR,
        WALL_SHELL_PREVIEW,
        WALL_SHELL_PREVIEW_END,
        WALL_RECOMPUTE_COLORS,
        WALL_REFRESH_OVERVIEW_BACKDROP,
        WALL_UPDATE_TAGS,
        WALL_UPDATE_ANALYSIS,
        WALL_HISTORY_BACK,
        WALL_HISTORY_FORWARD,
        WALL_HISTORY_LIST,
        WALL_PLAYLIST_NEXT,
        WALL_PLAYLIST_PREV,
        WALL_PLAYLIST_RELOAD,
        PLAYLIST_LIST,
        PLAYLIST_CREATE,
        PLAYLIST_DELETE,
        PLAYLIST_UPDATE,
        PLAYLIST_MEMBERS,
        PLAYLIST_MEMBERSHIPS,
        PLAYLIST_ADD,
        PLAYLIST_REMOVE,
        PLAYLIST_TOGGLE,
        PLAYLIST_MOVE,
        PLAYLIST_ASSIGN,
        PLAYLIST_STOP,
        SCHEDULE_RELOAD,
        WORKSPACE_RELOAD,
        WORKSPACE_LIST,
        SCAN_ITEM,
        SCAN_DONE,
        SCAN_REMOVED,
        REMOTE_THUMB,
        PREVIEW_READY,
        RECOMPUTE_PROGRESS,
        RECOMPUTE_DONE,
        EFFECTS_LIST,
        EFFECTS_PREVIEW,
        EFFECTS_COMMIT,
        EFFECTS_DISCARD,
        EFFECTS_BACKFILL_TAGS,
        THEME_BACKENDS,
        THEME_PREVIEW,
        THEME_PREVIEWS,
        TASK_LIST,
        TASK_CONTROL,
        OPTIMIZE_START,
        OPTIMIZE_STATUS,
        SOURCE_LIST,
        SOURCE_PREVIEW,
        SOURCE_DOWNLOAD,
        WALLHAVEN_SEARCH,
        WALLHAVEN_COLLECTIONS,
        WALLHAVEN_PREVIEW,
        WALLHAVEN_DOWNLOAD,
        STEAM_SEARCH,
        STEAM_PREVIEW,
        STEAM_DOWNLOAD,
    ];
}

#[cfg(test)]
#[path = "rpc_catalog_tests.rs"]
mod tests;
