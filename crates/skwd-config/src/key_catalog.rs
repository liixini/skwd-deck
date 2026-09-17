pub mod keys {
    pub mod playback {
        pub const PROCESS_ENABLED: &str = "playback.processEnabled";
        pub const PROCESSES: &str = "playback.processes";
        pub const FULLSCREEN: &str = "playback.fullscreen";
        pub const MAXIMIZED: &str = "playback.maximized";
        pub const FULLSCREEN_SCOPE: &str = "playback.fullscreenScope";
        pub const RESUME_DELAY: &str = "playback.resumeDelay";
        pub const MUTE_ON_OTHER_AUDIO: &str = "playback.muteOnOtherAudio";
    }
    pub mod wallpaper {
        pub const MUTE: &str = "wallpaperMute";
        pub const VOLUME: &str = "wallpaperVolume";
    }
    pub mod display {
        pub const FILL_COLOR: &str = "display.fillColor";
        pub const FILL_MODE: &str = "display.fillMode";
        pub const FILL_MODES: &str = "display.fillModes";
        pub const OUTPUT_LOCKS: &str = "display.outputLocks";
        pub const OUTPUT_POLICIES: &str = "display.outputPolicies";
    }
    pub mod effects {
        pub const AUTO_RECOLOR: &str = "effects.autoRecolor";
        pub const AUTO_THEME: &str = "effects.autoTheme";
    }
    pub mod dms {
        pub const HOVER_PREVIEW: &str = "dms.hoverPreview";
    }
    pub mod features {
        pub const MATUGEN: &str = "features.matugen";
        pub const STEAM: &str = "features.steam";
        pub const WALLHAVEN: &str = "features.wallhaven";
    }
    pub mod filter_bar {
        pub const DEFAULT_FOLDER: &str = "filterBar.defaultFolder";
        pub const LAST_COLOR: &str = "filterBar.last.color";
        pub const LAST_FAVOURITES_ONLY: &str = "filterBar.last.favouritesOnly";
        pub const LAST_FOLDER: &str = "filterBar.last.folder";
        pub const LAST_SHOW_HIDDEN_FOLDERS: &str = "filterBar.last.showHiddenFolders";
        pub const LAST_KIND: &str = "filterBar.last.kind";
        pub const LAST_ORIENT: &str = "filterBar.last.orient";
        pub const LAST_RESOLUTION: &str = "filterBar.last.resolution";
        pub const LAST_SORT: &str = "filterBar.last.sort";
        pub const OFFSET_X: &str = "filterBar.offsetX";
        pub const OFFSET_Y: &str = "filterBar.offsetY";
        pub const ORIENTATION: &str = "filterBar.orientation";
        pub const RESOLUTION_PRESETS: &str = "filterBar.resolutionPresets";
        pub const SHOW_COLORS: &str = "filterBar.show.colors";
        pub const SHOW_FAVOURITES: &str = "filterBar.show.favourites";
        pub const SHOW_FOLDER: &str = "filterBar.show.folder";
        pub const SHOW_RANDOM: &str = "filterBar.show.random";
        pub const SHOW_RESOLUTION: &str = "filterBar.show.resolution";
        pub const SHOW_TAG_CLOUD: &str = "filterBar.show.tagcloud";
        pub const SHOW_TAGGING: &str = "filterBar.show.tagging";
        pub const SHOW_THEME: &str = "filterBar.show.theme";
        pub const STICKY: &str = "filterBar.sticky";
        pub const VISUAL_STYLE: &str = "filterBar.visualStyle";
    }
    pub mod general {
        pub const APPLY_ON_PICKER_MONITOR: &str = "general.applyOnPickerMonitor";
        pub const CLOSE_ON_SELECTION: &str = "general.closeOnSelection";
        pub const FILTER_BAR_ALWAYS_VISIBLE: &str = "general.filterBarAlwaysVisible";
        pub const LOCALE: &str = "general.locale";
        pub const LANGUAGE: &str = "general.language";
        pub const MAX_FPS: &str = "general.maxFps";
        pub const NOTIFY_ON_WALLPAPER_CHANGE: &str = "general.notifyOnWallpaperChange";
        pub const OPEN_FADE_FROM: &str = "general.openFadeFrom";
        pub const RANDOM_INCLUDE_FAVOURITES: &str = "general.randomIncludeFavourites";
        pub const RANDOM_INCLUDE_STATIC: &str = "general.randomIncludeStatic";
        pub const RANDOM_INCLUDE_VIDEO: &str = "general.randomIncludeVideo";
        pub const RANDOM_INCLUDE_WE: &str = "general.randomIncludeWE";
        pub const RANDOM_INTERVAL: &str = "general.randomInterval";
        pub const RANDOM_ROTATE: &str = "general.randomRotate";
        pub const SETTINGS_STYLE: &str = "general.settingsStyle";
        pub const UI_SCALE: &str = "general.uiScale";
        pub const WEATHER_MATCH: &str = "general.weatherMatch";
    }
    pub mod system {
        pub const EXTERNAL_MATUGEN_COMMAND: &str = "externalMatugenCommand";
        pub const MONITOR: &str = "monitor";
        pub const PICK_ONLY_MODE: &str = "pickOnlyMode";
        pub const POST_PROCESS_ON_RESTORE: &str = "postProcessOnRestore";
        pub const RESTORE_ON_STARTUP: &str = "restoreOnStartup";
    }
    pub mod launch {
        pub const ANIMATION: &str = "launch.animation";
    }
    pub mod motion {
        pub const FAST_MS: &str = "motion.fastMs";
        pub const STANDARD_MS: &str = "motion.standardMs";
        pub const SLOW_MS: &str = "motion.slowMs";
        pub const LAUNCH_SPEED: &str = "motion.launchSpeed";
        pub const FILTER_SWAP_SPEED: &str = "motion.filterSwapSpeed";
    }
    pub mod history {
        pub const DEPTH: &str = "history.depth";
        pub const ENABLED: &str = "history.enabled";
    }
    pub mod integrations {
        pub const LIST: &str = "integrations";
    }
    pub mod library {
        pub const POLLING_FALLBACK: &str = "library.pollingFallback";
        pub const POLLING_INTERVAL_SECONDS: &str = "library.pollingIntervalSeconds";
    }
    pub mod matugen {
        pub const PREFIX: &str = "matugen.";
        pub const DEFAULT_CONFIG: &str = "defaultMatugenConfig";
        pub const COLOR_INDEX: &str = "matugen.colorIndex";
        pub const CONTRAST: &str = "matugen.contrast";
        pub const MODE: &str = "matugen.mode";
        pub const SCHEME_TYPE: &str = "matugen.schemeType";
    }
    pub mod niri {
        pub const FULL_WIDTH_PAUSE: &str = "niri.fullWidthPause";
        pub const OVERVIEW_ONLY_PLAYBACK: &str = "niri.overviewOnlyPlayback";
        pub const BACKDROP: &str = "niri.backdrop";
        pub const BACKDROP_AUTO_THEME: &str = "niri.backdropAutoTheme";
        pub const BACKDROP_DIM: &str = "niri.backdropDim";
        pub const BACKDROP_FOLLOW_WALLPAPER: &str = "niri.backdropFollowWallpaper";
        pub const BACKDROP_THEME: &str = "niri.backdropTheme";
        pub const OVERVIEW_BACKDROP: &str = "niri.overviewBackdrop";
        pub const OVERVIEW_BACKDROP_BLUR: &str = "niri.overviewBackdropBlur";
        pub const OVERVIEW_BACKDROP_BLUR_ENABLED: &str = "niri.overviewBackdropBlurEnabled";
    }
    pub mod noctalia {
        pub const HOVER_PREVIEW: &str = "noctalia.hoverPreview";
        pub const THEME_MODE: &str = "noctalia.themeMode";
    }
    pub mod plasma {
        pub const LOCK_SCREEN_DYNAMIC: &str = "plasma.lockScreen.dynamic";
        pub const LOCK_SCREEN_IMAGE: &str = "plasma.lockScreen.image";
        pub const LOCK_SCREEN_MODE: &str = "plasma.lockScreen.mode";
    }
    pub mod paper {
        pub const WALLPAPER_LAYER: &str = "paper.wallpaperLayer";
        pub const AWWW_INVERT_Y: &str = "paper.awww.invertY";
        pub const AWWW_TRANSITION_ANGLE: &str = "paper.awww.transitionAngle";
        pub const AWWW_TRANSITION_BEZIER: &str = "paper.awww.transitionBezier";
        pub const AWWW_TRANSITION_DURATION_MS: &str = "paper.awww.transitionDurationMs";
        pub const AWWW_TRANSITION_FPS: &str = "paper.awww.transitionFps";
        pub const AWWW_TRANSITION_POS: &str = "paper.awww.transitionPos";
        pub const AWWW_TRANSITION_STEP: &str = "paper.awww.transitionStep";
        pub const AWWW_TRANSITION_TYPE: &str = "paper.awww.transitionType";
        pub const AWWW_TRANSITION_WAVE_HEIGHT: &str = "paper.awww.transitionWaveHeight";
        pub const AWWW_TRANSITION_WAVE_WIDTH: &str = "paper.awww.transitionWaveWidth";
        pub const AWWW_FILTER: &str = "paper.awww.filter";
        pub const ENGINE: &str = "paper.engine";
        pub const IDLE_PAUSE_SECONDS: &str = "paper.idlePauseSeconds";
        pub const PERFORMANCE_MODE: &str = "paper.performanceMode";
        pub const VIDEO_MULTI_PROCESS: &str = "paper.videoMultiProcess";
        pub const VIDEO_ENGINE: &str = "paper.videoEngine";
    }
    pub mod paths {
        pub const PAPER_BIN: &str = "paths.paperBin";
        pub const PAPER_STILL_BIN: &str = "paths.paperStillBin";
        pub const PAPER_VK_BIN: &str = "paths.paperVkBin";
        pub const STEAM_WE_ASSETS: &str = "paths.steamWeAssets";
        pub const DMS_BIN: &str = "paths.dmsBin";
        pub const CAELESTIA_BIN: &str = "paths.caelestiaBin";
        pub const NOCTALIA_BIN: &str = "paths.noctaliaBin";
        pub const CACHE: &str = "paths.cache";
        pub const STEAM: &str = "paths.steam";
        pub const STEAM_WORKSHOP: &str = "paths.steamWorkshop";
        pub const VIDEO_WALLPAPER: &str = "paths.videoWallpaper";
        pub const WALLPAPER: &str = "paths.wallpaper";
        pub const TEMPLATES: &str = "paths.templates";
    }
    pub mod playlist {
        pub const PREFIX: &str = "playlist.";
        pub const ASSIGN: &str = "playlist.assign";
        pub const LISTS: &str = "playlist.lists";
    }
    pub mod post_processing {
        pub const LIST: &str = "postProcessing";
    }
    pub mod schedule {
        pub const PREFIX: &str = "schedule.";
        pub const LATITUDE: &str = "schedule.latitude";
        pub const LONGITUDE: &str = "schedule.longitude";
        pub const APPLY_ON_START: &str = "schedule.applyOnStart";
        pub const DAY_MODE: &str = "schedule.dayMode";
        pub const DAY_SET: &str = "schedule.daySet";
        pub const DAY_TIME: &str = "schedule.dayTime";
        pub const ENABLED: &str = "schedule.enabled";
        pub const ENTRIES: &str = "schedule.entries";
        pub const MIGRATED: &str = "schedule.migrated";
        pub const NIGHT_MODE: &str = "schedule.nightMode";
        pub const NIGHT_SET: &str = "schedule.nightSet";
        pub const NIGHT_TIME: &str = "schedule.nightTime";
        pub const RULES: &str = "schedule.rules";
        pub const TRIGGER: &str = "schedule.trigger";
    }
    pub mod selector {
        pub const START_POSITION: &str = "components.wallpaperSelector.startPosition";
        pub const LAST_APPLIED_KEY: &str = "components.wallpaperSelector.lastAppliedKey";
        pub const LAST_BROWSE_POSITION: &str = "components.wallpaperSelector.lastBrowsePosition";
        pub const EXPANDED_WIDTH: &str = "components.wallpaperSelector.expandedWidth";
        pub const GRID_COLUMNS: &str = "components.wallpaperSelector.gridColumns";
        pub const GRID_BORDER_WIDTH: &str = "components.wallpaperSelector.gridBorderWidth";
        pub const GRID_CYLINDER_BEND: &str = "components.wallpaperSelector.gridCylinderBend";
        pub const GRID_CYLINDER_RADIUS: &str = "components.wallpaperSelector.gridCylinderRadius";
        pub const GRID_FILTER_BAR_OFFSET_X: &str =
            "components.wallpaperSelector.gridFilterBarOffsetX";
        pub const GRID_FILTER_BAR_OFFSET_Y: &str =
            "components.wallpaperSelector.gridFilterBarOffsetY";
        pub const GRID_CORNER_RADIUS: &str = "components.wallpaperSelector.gridCornerRadius";
        pub const GRID_GAP_X: &str = "components.wallpaperSelector.gridGapX";
        pub const GRID_GAP_Y: &str = "components.wallpaperSelector.gridGapY";
        pub const GRID_LAYOUT: &str = "components.wallpaperSelector.gridLayout";
        pub const GRID_FLOW_FREQUENCY: &str = "components.wallpaperSelector.gridFlowFrequency";
        pub const GRID_FLOW_WAVE: &str = "components.wallpaperSelector.gridFlowWave";
        pub const GRID_ROUND_CORNERS: &str = "components.wallpaperSelector.gridRoundCorners";
        pub const GRID_ROWS: &str = "components.wallpaperSelector.gridRows";
        pub const GRID_SEARCH_PANEL_OFFSET_X: &str =
            "components.wallpaperSelector.gridSearchPanelOffsetX";
        pub const GRID_SEARCH_PANEL_OFFSET_Y: &str =
            "components.wallpaperSelector.gridSearchPanelOffsetY";
        pub const GRID_SELECTED_SCALE: &str = "components.wallpaperSelector.gridSelectedScale";
        pub const GRID_SCALE_VARIANCE: &str = "components.wallpaperSelector.gridScaleVariance";
        pub const GRID_SCATTER: &str = "components.wallpaperSelector.gridScatter";
        pub const GRID_STAGE_DEPTH_ANGLE: &str = "components.wallpaperSelector.gridStageDepthAngle";
        pub const GRID_STAGE_PERSPECTIVE: &str =
            "components.wallpaperSelector.gridStagePerspective";
        pub const GRID_STAGE_ROTATION: &str = "components.wallpaperSelector.gridStageRotation";
        pub const GRID_STAGE_SCALE: &str = "components.wallpaperSelector.gridStageScale";
        pub const GRID_STAGE_SHEAR_X: &str = "components.wallpaperSelector.gridStageShearX";
        pub const GRID_STAGE_SHEAR_Y: &str = "components.wallpaperSelector.gridStageShearY";
        pub const GRID_STAGE_X: &str = "components.wallpaperSelector.gridStageX";
        pub const GRID_STAGE_Y: &str = "components.wallpaperSelector.gridStageY";
        pub const GRID_STAGGER: &str = "components.wallpaperSelector.gridStagger";
        pub const GRID_THUMB_HEIGHT: &str = "components.wallpaperSelector.gridThumbHeight";
        pub const GRID_THUMB_WIDTH: &str = "components.wallpaperSelector.gridThumbWidth";
        pub const HAND_ARCH: &str = "components.wallpaperSelector.handArch";
        pub const HAND_BACKDROP: &str = "components.wallpaperSelector.handBackdrop";
        pub const HAND_BACKDROP_BLUR: &str = "components.wallpaperSelector.handBackdropBlur";
        pub const HAND_CORNER_RADIUS: &str = "components.wallpaperSelector.handCornerRadius";
        pub const HAND_FAN_ANGLE: &str = "components.wallpaperSelector.handFanAngle";
        pub const HAND_FAN_ROLL: &str = "components.wallpaperSelector.handFanRoll";
        pub const HAND_SKEW: &str = "components.wallpaperSelector.handSkew";
        pub const HAND_BOB: &str = "components.wallpaperSelector.handBob";
        pub const HAND_CARD_HEIGHT: &str = "components.wallpaperSelector.handCardHeight";
        pub const HAND_CARD_WIDTH: &str = "components.wallpaperSelector.handCardWidth";
        pub const HAND_COUNT: &str = "components.wallpaperSelector.handCount";
        pub const HAND_CUT: &str = "components.wallpaperSelector.handCut";
        pub const HAND_CUT_VARIANCE: &str = "components.wallpaperSelector.handCutVariance";
        pub const HAND_GHOSTS: &str = "components.wallpaperSelector.handGhosts";
        pub const HAND_MOVE: &str = "components.wallpaperSelector.handMove";
        pub const HAND_MOVE_CASCADE: &str = "components.wallpaperSelector.handMoveCascade";
        pub const HAND_MOVE_CORKSCREW: &str = "components.wallpaperSelector.handMoveCorkscrew";
        pub const HAND_MOVE_RIBBON: &str = "components.wallpaperSelector.handMoveRibbon";
        pub const HAND_MOVE_SHUFFLE: &str = "components.wallpaperSelector.handMoveShuffle";
        pub const HAND_MOVE_SPIRAL: &str = "components.wallpaperSelector.handMoveSpiral";
        pub const HAND_PERSPECTIVE: &str = "components.wallpaperSelector.handPerspective";
        pub const HAND_RIBBON_AXIS: &str = "components.wallpaperSelector.handRibbonAxis";
        pub const HAND_RIBBONS: &str = "components.wallpaperSelector.handRibbons";
        pub const HAND_SPEED: &str = "components.wallpaperSelector.handSpeed";
        pub const HAND_SPREAD: &str = "components.wallpaperSelector.handSpread";
        pub const HAND_STAGE_X: &str = "components.wallpaperSelector.handStageX";
        pub const HAND_STAGE_Y: &str = "components.wallpaperSelector.handStageY";
        pub const HAND_TILT: &str = "components.wallpaperSelector.handTilt";
        pub const HEX_ARC_INTENSITY_X10: &str = "components.wallpaperSelector.hexArcIntensityX10";
        pub const HEX_ASPECT: &str = "components.wallpaperSelector.hexAspect";
        pub const HEX_COLS: &str = "components.wallpaperSelector.hexCols";
        pub const HEX_CURVE: &str = "components.wallpaperSelector.hexCurve";
        pub const HEX_CURVE_FREQUENCY: &str = "components.wallpaperSelector.hexCurveFrequency";
        pub const HEX_FILTER_BAR_OFFSET_X: &str =
            "components.wallpaperSelector.hexFilterBarOffsetX";
        pub const HEX_FILTER_BAR_OFFSET_Y: &str =
            "components.wallpaperSelector.hexFilterBarOffsetY";
        pub const HEX_GAP_X: &str = "components.wallpaperSelector.hexGapX";
        pub const HEX_GAP_Y: &str = "components.wallpaperSelector.hexGapY";
        pub const HEX_LENS: &str = "components.wallpaperSelector.hexLens";
        pub const HEX_LENS_RADIUS: &str = "components.wallpaperSelector.hexLensRadius";
        pub const HEX_ORBIT: &str = "components.wallpaperSelector.hexOrbit";
        pub const HEX_ORBIT_RADIUS: &str = "components.wallpaperSelector.hexOrbitRadius";
        pub const HEX_RADIUS: &str = "components.wallpaperSelector.hexRadius";
        pub const HEX_ROWS: &str = "components.wallpaperSelector.hexRows";
        pub const HEX_SCROLL_STEP: &str = "components.wallpaperSelector.hexScrollStep";
        pub const HEX_SEARCH_PANEL_OFFSET_X: &str =
            "components.wallpaperSelector.hexSearchPanelOffsetX";
        pub const HEX_SEARCH_PANEL_OFFSET_Y: &str =
            "components.wallpaperSelector.hexSearchPanelOffsetY";
        pub const HEX_SHAPE: &str = "components.wallpaperSelector.hexShape";
        pub const HEX_SCATTER: &str = "components.wallpaperSelector.hexScatter";
        pub const HEX_STAGE_DEPTH_ANGLE: &str = "components.wallpaperSelector.hexStageDepthAngle";
        pub const HEX_STAGE_PERSPECTIVE: &str = "components.wallpaperSelector.hexStagePerspective";
        pub const HEX_STAGE_ROTATION: &str = "components.wallpaperSelector.hexStageRotation";
        pub const HEX_STAGE_SCALE: &str = "components.wallpaperSelector.hexStageScale";
        pub const HEX_STAGE_SHEAR_X: &str = "components.wallpaperSelector.hexStageShearX";
        pub const HEX_STAGE_SHEAR_Y: &str = "components.wallpaperSelector.hexStageShearY";
        pub const HEX_STAGE_X: &str = "components.wallpaperSelector.hexStageX";
        pub const HEX_STAGE_Y: &str = "components.wallpaperSelector.hexStageY";
        pub const HEX_STAGGER: &str = "components.wallpaperSelector.hexStagger";
        pub const HEX_TWIST: &str = "components.wallpaperSelector.hexTwist";
        pub const SANDY_ARC: &str = "components.wallpaperSelector.sandyArc";
        pub const SANDY_BLEND: &str = "components.wallpaperSelector.sandyBlend";
        pub const SANDY_CENTER: &str = "components.wallpaperSelector.sandyCenter";
        pub const SANDY_DURATION: &str = "components.wallpaperSelector.sandyDuration";
        pub const SANDY_EDGE_SPEED: &str = "components.wallpaperSelector.sandyEdgeSpeed";
        pub const SANDY_FAN: &str = "components.wallpaperSelector.sandyFan";
        pub const SANDY_FILTER_BAR_OFFSET_X: &str =
            "components.wallpaperSelector.sandyFilterBarOffsetX";
        pub const SANDY_FILTER_BAR_OFFSET_Y: &str =
            "components.wallpaperSelector.sandyFilterBarOffsetY";
        pub const SANDY_FRONT: &str = "components.wallpaperSelector.sandyFront";
        pub const SANDY_GRAIN: &str = "components.wallpaperSelector.sandyGrain";
        pub const SANDY_LOD: &str = "components.wallpaperSelector.sandyLod";
        pub const SANDY_LOD_AUTO: &str = "components.wallpaperSelector.sandyLodAuto";
        pub const SANDY_ORBIT: &str = "components.wallpaperSelector.sandyOrbit";
        pub const SANDY_RES_SCALE: &str = "components.wallpaperSelector.sandyResScale";
        pub const SANDY_RING_BLEND: &str = "components.wallpaperSelector.sandyRingBlend";
        pub const SANDY_RING_HOLD: &str = "components.wallpaperSelector.sandyRingHold";
        pub const SANDY_RING_SIZE: &str = "components.wallpaperSelector.sandyRingSize";
        pub const SANDY_RING_SOFT: &str = "components.wallpaperSelector.sandyRingSoft";
        pub const SANDY_RING_SPIN: &str = "components.wallpaperSelector.sandyRingSpin";
        pub const SANDY_RING_WAVE: &str = "components.wallpaperSelector.sandyRingWave";
        pub const SANDY_SEARCH_PANEL_OFFSET_X: &str =
            "components.wallpaperSelector.sandySearchPanelOffsetX";
        pub const SANDY_SEARCH_PANEL_OFFSET_Y: &str =
            "components.wallpaperSelector.sandySearchPanelOffsetY";
        pub const SANDY_STAGE_X: &str = "components.wallpaperSelector.sandyStageX";
        pub const SANDY_STAGE_Y: &str = "components.wallpaperSelector.sandyStageY";
        pub const SANDY_SKEW: &str = "components.wallpaperSelector.sandySkew";
        pub const SANDY_SLICE_HEIGHT: &str = "components.wallpaperSelector.sandySliceHeight";
        pub const SANDY_SLICE_WIDTH: &str = "components.wallpaperSelector.sandySliceWidth";
        pub const SANDY_SPACING: &str = "components.wallpaperSelector.sandySpacing";
        pub const SANDY_STRANDS: &str = "components.wallpaperSelector.sandyStrands";
        pub const SANDY_TURBULENCE: &str = "components.wallpaperSelector.sandyTurbulence";
        pub const SANDY_TWIST: &str = "components.wallpaperSelector.sandyTwist";
        pub const SANDY_WAIST: &str = "components.wallpaperSelector.sandyWaist";
        pub const SKEW_OFFSET: &str = "components.wallpaperSelector.skewOffset";
        pub const SLICE_FILTER_BAR_OFFSET_X: &str =
            "components.wallpaperSelector.sliceFilterBarOffsetX";
        pub const SLICE_FILTER_BAR_OFFSET_Y: &str =
            "components.wallpaperSelector.sliceFilterBarOffsetY";
        pub const SLICE_EDGE_TILT: &str = "components.wallpaperSelector.sliceEdgeTilt";
        pub const SLICE_HEIGHT: &str = "components.wallpaperSelector.sliceHeight";
        pub const SLICE_SEARCH_PANEL_OFFSET_X: &str =
            "components.wallpaperSelector.sliceSearchPanelOffsetX";
        pub const SLICE_SEARCH_PANEL_OFFSET_Y: &str =
            "components.wallpaperSelector.sliceSearchPanelOffsetY";
        pub const SLICE_STAGE_X: &str = "components.wallpaperSelector.sliceStageX";
        pub const SLICE_STAGE_Y: &str = "components.wallpaperSelector.sliceStageY";
        pub const SLICE_SPACING: &str = "components.wallpaperSelector.sliceSpacing";
        pub const SLICE_WOBBLE_STRENGTH: &str = "components.wallpaperSelector.sliceWobbleStrength";
        pub const STEAM_COLUMNS: &str = "components.wallpaperSelector.steamColumns";
        pub const STEAM_ROWS: &str = "components.wallpaperSelector.steamRows";
        pub const STEAM_THUMB_HEIGHT: &str = "components.wallpaperSelector.steamThumbHeight";
        pub const STEAM_THUMB_WIDTH: &str = "components.wallpaperSelector.steamThumbWidth";
        pub const DOWNLOADER_WALL_COLUMNS: &str =
            "components.wallpaperSelector.downloaderWallColumns";
        pub const DOWNLOADER_WALL_ROWS: &str = "components.wallpaperSelector.downloaderWallRows";
        pub const DOWNLOADER_WALL_THUMB_HEIGHT: &str =
            "components.wallpaperSelector.downloaderWallThumbHeight";
        pub const DOWNLOADER_WALL_THUMB_WIDTH: &str =
            "components.wallpaperSelector.downloaderWallThumbWidth";
        pub const DOWNLOADER_WALL_GAP_X: &str = "components.wallpaperSelector.downloaderWallGapX";
        pub const DOWNLOADER_WALL_GAP_Y: &str = "components.wallpaperSelector.downloaderWallGapY";
        pub const DOWNLOADER_WALL_CORNER_RADIUS: &str =
            "components.wallpaperSelector.downloaderWallCornerRadius";
        pub const DOWNLOADER_WALL_BORDER_WIDTH: &str =
            "components.wallpaperSelector.downloaderWallBorderWidth";
        pub const TAG_CLOUD_HEIGHT: &str = "components.wallpaperSelector.tagCloudHeight";
        pub const TAG_CLOUD_OFFSET_X: &str = "components.wallpaperSelector.tagCloudOffsetX";
        pub const TAG_CLOUD_OFFSET_Y: &str = "components.wallpaperSelector.tagCloudOffsetY";
        pub const TAG_CLOUD_ROWS: &str = "components.wallpaperSelector.tagCloudRows";
        pub const VISIBLE_COUNT: &str = "components.wallpaperSelector.visibleCount";
        pub const WALLHAVEN_COLUMNS: &str = "components.wallpaperSelector.wallhavenColumns";
        pub const WALLHAVEN_ROWS: &str = "components.wallpaperSelector.wallhavenRows";
        pub const WALLHAVEN_THUMB_HEIGHT: &str =
            "components.wallpaperSelector.wallhavenThumbHeight";
        pub const WALLHAVEN_THUMB_WIDTH: &str = "components.wallpaperSelector.wallhavenThumbWidth";
        pub const LIVE_PREVIEW: &str = "selector.livePreview";
        pub const SHOW_TYPE_BADGES: &str = "components.wallpaperSelector.showTypeBadges";
        pub const PRESET_DRAFT_NAME: &str = "components.wallpaperSelector.presetDraftName";
        pub const COMPONENTS: &str = "components.wallpaperSelector";
        pub const CORNER_BL: &str = "components.wallpaperSelector.cornerBL";
        pub const CORNER_BR: &str = "components.wallpaperSelector.cornerBR";
        pub const CORNER_RADIUS: &str = "components.wallpaperSelector.cornerRadius";
        pub const CORNER_TL: &str = "components.wallpaperSelector.cornerTL";
        pub const CORNER_TR: &str = "components.wallpaperSelector.cornerTR";
        pub const DISPLAY_MODE: &str = "components.wallpaperSelector.displayMode";
        pub const ENABLED: &str = "components.wallpaperSelector.enabled";
        pub const FLIP_BACK_REVEAL: &str = "components.wallpaperSelector.flipBackReveal";
        pub const FLIP_DURATION_MS: &str = "components.wallpaperSelector.flipDurationMs";
        pub const FLIP_EFFECT: &str = "components.wallpaperSelector.flipEffect";
        pub const FLIP_SHADER: &str = "components.wallpaperSelector.flipShader";
        pub const HEX_ARC: &str = "components.wallpaperSelector.hexArc";
        pub const HEX_ARC_INTENSITY: &str = "components.wallpaperSelector.hexArcIntensity";
        pub const ROUND_CORNERS: &str = "components.wallpaperSelector.roundCorners";
        pub const SANDY_OUTGOING_LIVE: &str = "components.wallpaperSelector.sandyOutgoingLive";
        pub const SANDY_SWAP_LOOP: &str = "components.wallpaperSelector.sandySwapLoop";
        pub const SANDY_SWAP_STYLE: &str = "components.wallpaperSelector.sandySwapStyle";
        pub const SANDY_VORTEX: &str = "components.wallpaperSelector.sandyVortex";
        pub const SLICE_WIDTH: &str = "components.wallpaperSelector.sliceWidth";
        pub const SLICE_WOBBLE: &str = "components.wallpaperSelector.sliceWobble";
        pub const TAG_CLOUD_WIDTH: &str = "components.wallpaperSelector.tagCloudWidth";
    }
    pub mod tagging {
        pub const DEFAULT_SEARCH_MODE: &str = "tagging.defaultSearchMode";
    }
    pub mod semantic {
        pub const INDEX_PROFILE: &str = "semantic.indexProfile";
        pub const MANIFEST: &str = "semantic.manifest";
        pub const MODELS: &str = "semantic.models";
    }
    pub mod sources {
        pub const WALLHAVEN_SHOW_APPLY_BUTTON: &str = "sources.wallhaven.showApplyButton";
        pub const STEAM_SHOW_APPLY_BUTTON: &str = "sources.steam.showApplyButton";
        pub const UNSPLASH_SHOW_APPLY_BUTTON: &str = "sources.unsplash.showApplyButton";
        pub const PEXELS_SHOW_APPLY_BUTTON: &str = "sources.pexels.showApplyButton";
        pub const YOUTUBE_SHOW_APPLY_BUTTON: &str = "sources.youtube.showApplyButton";
        pub const YOUTUBE_MAX_HEIGHT: &str = "sources.youtube.maxHeight";
        pub const BING_ENABLED: &str = "sources.bing.enabled";
        pub const BING_MARKET: &str = "sources.bing.market";
        pub const PEXELS_API_KEY: &str = "sources.pexels.apiKey";
        pub const PEXELS_ENABLED: &str = "sources.pexels.enabled";
        pub const UNSPLASH_ACCESS_KEY: &str = "sources.unsplash.accessKey";
        pub const UNSPLASH_ENABLED: &str = "sources.unsplash.enabled";
        pub const YOUTUBE_ENABLED: &str = "sources.youtube.enabled";
        pub const YOUTUBE_MAX_MINUTES: &str = "sources.youtube.maxMinutes";
    }
    pub mod steam {
        pub const DEFAULT_SORT: &str = "steam.defaults.sort";
        pub const DEFAULT_TREND_DAYS: &str = "steam.defaults.trendDays";
        pub const DEFAULT_TYPE: &str = "steam.defaults.type";
        pub const DEFAULT_CATEGORY: &str = "steam.defaults.category";
        pub const DEFAULT_RESOLUTION: &str = "steam.defaults.resolution";
        pub const DEFAULT_NSFW: &str = "steam.defaults.nsfw";

        pub const API_KEY: &str = "steam.apiKey";
        pub const BACKEND: &str = "steam.backend";
        pub const USERNAME: &str = "steam.username";
    }
    pub mod theme {
        pub const PREFIX: &str = "theme.";
        pub const BACKEND: &str = "theme.backend";
        pub const POLICY: &str = "theme.policy";
        pub const AUTHORITY: &str = "theme.authority";
        pub const ENGINE: &str = "theme.engine";
        pub const CUSTOM_COLORS: &str = "theme.customColors";
        pub const MODE: &str = "theme.mode";
        pub const SCHEME: &str = "theme.scheme";
        pub const STYLE: &str = "theme.style";
        pub const TARGETS: &str = "theme.targets";
        pub const NATIVE_COLORS_PATH: &str = "theme.nativeColorsPath";
        pub const NATIVE_TEMPLATES: &str = "theme.nativeTemplates";
        pub const NOCTALIA_PURE_BLACK: &str = "theme.noctaliaPureBlack";
        pub const NOCTALIA_SCHEME: &str = "theme.noctaliaScheme";
        pub const PYWAL_SATURATE: &str = "theme.pywalSaturate";
        pub const WALLPAPER_PROFILES: &str = "theme.wallpaperProfiles";
        pub const SAVED_THEMES: &str = "theme.savedThemes";
        pub const STATIC_THEME: &str = "theme.staticTheme";
        pub const WALLUST_COLORSPACE: &str = "theme.wallustColorspace";
        pub const WALLUST_PALETTE: &str = "theme.wallustPalette";
    }
    pub mod transition {
        pub const DURATION_MS: &str = "transition.durationMs";
        pub const ENABLED: &str = "transition.enabled";
        pub const FAMILY: &str = "transition.family";
        pub const PREVIEW: &str = "transition.preview";
        pub const PREVIEW_FPS: &str = "transition.previewFps";
        pub const SAND_FPS: &str = "transition.sandFps";
        pub const SAND_PRIMARY: &str = "transition.sandPrimary";
        pub const SAND_QUALITY: &str = "transition.sandQuality";
        pub const SAND_SCOPE: &str = "transition.sandScope";
        pub const SAND_SHARP: &str = "transition.sandSharp";
        pub const SHADER: &str = "transition.shader";
        pub const SHADER_SCOPES: &str = "transition.shaderScopes";
    }
    pub mod video_optimize {
        pub const CODEC: &str = "videoOptimize.codec";
        pub const ENABLED: &str = "videoOptimize.enabled";
        pub const FIT_OUTPUTS: &str = "videoOptimize.fitOutputs";
        pub const HW_ENCODE: &str = "videoOptimize.hwEncode";
        pub const MAX_FPS: &str = "videoOptimize.maxFps";
        pub const MAX_HEIGHT: &str = "videoOptimize.maxHeight";
    }
    pub mod video_preview {
        pub const DELAY_MS: &str = "videoPreview.delayMs";
        pub const ENABLED: &str = "videoPreview.enabled";
        pub const MODE: &str = "videoPreview.mode";
    }
    pub mod vitals {
        pub const ENABLED: &str = "vitals.enabled";
        pub const INTERVAL_MINS: &str = "vitals.intervalMins";
    }
    pub mod wallhaven {
        pub const DEFAULT_SORT: &str = "wallhaven.defaults.sort";
        pub const DEFAULT_TOP_RANGE: &str = "wallhaven.defaults.topRange";
        pub const DEFAULT_ATLEAST: &str = "wallhaven.defaults.atleast";
        pub const DEFAULT_ATMOST: &str = "wallhaven.defaults.atmost";
        pub const DEFAULT_RATIOS: &str = "wallhaven.defaults.ratios";
        pub const DEFAULT_GENERAL: &str = "wallhaven.defaults.general";
        pub const DEFAULT_ANIME: &str = "wallhaven.defaults.anime";
        pub const DEFAULT_PEOPLE: &str = "wallhaven.defaults.people";
        pub const DEFAULT_SFW: &str = "wallhaven.defaults.sfw";
        pub const DEFAULT_SKETCHY: &str = "wallhaven.defaults.sketchy";
        pub const DEFAULT_NSFW: &str = "wallhaven.defaults.nsfw";

        pub const COLLECTIONS: &str = "wallhaven.collections";
        pub const API_KEY: &str = "wallhaven.apiKey";
        pub const USERNAME: &str = "wallhaven.username";
    }
    pub mod we_render {
        pub const PREFIX: &str = "weRender.";
        pub const CLAMP: &str = "weRender.clamp";
        pub const DISABLE_PARTICLES: &str = "weRender.disableParticles";
        pub const ENGINE: &str = "weRender.engine";
        pub const FPS: &str = "weRender.fps";
        pub const NATIVE: &str = "weRender.native";
        pub const SCALING: &str = "weRender.scaling";
    }
    pub mod workspace {
        pub const PREFIX: &str = "workspace.";
        pub const DEBOUNCE_MS: &str = "workspace.debounceMs";
        pub const ENABLED: &str = "workspace.enabled";
        pub const SLIDE_MS: &str = "workspace.slideMs";
        pub const WALLPAPERS: &str = "workspace.wallpapers";
    }
    pub mod keybind {
        pub const PREFIX: &str = "keys.";
        pub const APPLY: &str = "keys.apply";
        pub const AUTOCOMPLETE: &str = "keys.autocomplete";
        pub const COLOR_NEXT: &str = "keys.colorNext";
        pub const COLOR_PREV: &str = "keys.colorPrev";
        pub const EFFECTS: &str = "keys.effects";
        pub const FAVOURITE: &str = "keys.favourite";
        pub const FILTER_BAR: &str = "keys.filterBar";
        pub const FOLDER_PREV: &str = "keys.folderPrev";
        pub const FOLDER_NEXT: &str = "keys.folderNext";
        pub const FOLDER_TOGGLE: &str = "keys.folderToggle";
        pub const HIDDEN_FOLDERS: &str = "keys.hiddenFolders";
        pub const FLIP: &str = "keys.flip";
        pub const HELP: &str = "keys.help";
        pub const SCENE_PROPERTIES: &str = "keys.sceneProperties";
        pub const SELECT: &str = "keys.select";
        pub const STUDIO: &str = "keys.studio";
        pub const NAV_DOWN: &str = "keys.navDown";
        pub const NAV_LEFT: &str = "keys.navLeft";
        pub const NAV_RIGHT: &str = "keys.navRight";
        pub const NAV_UP: &str = "keys.navUp";
        pub const PLAYLISTS: &str = "keys.playlists";
        pub const SETTINGS: &str = "keys.settings";
        pub const TAG_CLOUD: &str = "keys.tagCloud";
        pub const TAG_MODE: &str = "keys.tagMode";
        pub const THEME_PANEL: &str = "keys.themePanel";
        pub const DOWNLOADS: &str = "keys.downloads";
        pub const RANDOM_ROTATE: &str = "keys.randomRotate";
        pub const SEARCH_MODE: &str = "keys.searchMode";
        pub const SORT_NEXT: &str = "keys.sortNext";
        pub const SORT_PREV: &str = "keys.sortPrev";
        pub const SOURCE_BING: &str = "keys.sourceBing";
        pub const SOURCE_PEXELS: &str = "keys.sourcePexels";
        pub const SOURCE_STEAM: &str = "keys.sourceSteam";
        pub const SOURCE_UNSPLASH: &str = "keys.sourceUnsplash";
        pub const SOURCE_WALLHAVEN: &str = "keys.sourceWallhaven";
        pub const SOURCE_YOUTUBE: &str = "keys.sourceYoutube";
        pub const TYPE_NEXT: &str = "keys.typeNext";
        pub const TYPE_PREV: &str = "keys.typePrev";
    }
    pub mod performance {
        pub const AUTO_DELETE_IMAGE_TRASH: &str = "performance.autoDeleteImageTrash";
        pub const AUTO_OPTIMIZE_IMAGES: &str = "performance.autoOptimizeImages";
        pub const BATTERY_FPS: &str = "performance.batteryFps";
        pub const BATTERY_SAVER: &str = "performance.batterySaver";
        pub const BATTERY_VIDEO_IDLE_SECONDS: &str = "performance.batteryVideoIdleSeconds";
        pub const BATTERY_WALLPAPER_PERFORMANCE: &str = "performance.batteryWallpaperPerformance";
        pub const GPU_DEVICE: &str = "performance.gpuDevice";
        pub const GPU_PREFERENCE: &str = "performance.gpuPreference";
        pub const IMAGE_OPTIMIZE_PRESET: &str = "performance.imageOptimizePreset";
        pub const IMAGE_OPTIMIZE_RESOLUTION: &str = "performance.imageOptimizeResolution";
        pub const IMAGE_TRASH_DAYS: &str = "performance.imageTrashDays";
        pub const MAX_THUMB_JOBS: &str = "performance.maxThumbJobs";
    }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
