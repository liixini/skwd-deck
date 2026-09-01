#![cfg(test)]

use super::*;

#[test]
fn wire_strings_pinned() {
    use super::keys;
    assert_eq!(keys::display::FILL_COLOR, "display.fillColor");
    assert_eq!(keys::display::FILL_MODE, "display.fillMode");
    assert_eq!(keys::display::FILL_MODES, "display.fillModes");
    assert_eq!(keys::display::OUTPUT_LOCKS, "display.outputLocks");
    assert_eq!(keys::dms::HOVER_PREVIEW, "dms.hoverPreview");
    assert_eq!(keys::features::MATUGEN, "features.matugen");
    assert_eq!(keys::features::STEAM, "features.steam");
    assert_eq!(keys::features::WALLHAVEN, "features.wallhaven");
    assert_eq!(keys::filter_bar::DEFAULT_FOLDER, "filterBar.defaultFolder");
    assert_eq!(keys::filter_bar::LAST_COLOR, "filterBar.last.color");
    assert_eq!(keys::filter_bar::LAST_FAVOURITES_ONLY, "filterBar.last.favouritesOnly");
    assert_eq!(keys::filter_bar::LAST_FOLDER, "filterBar.last.folder");
    assert_eq!(keys::filter_bar::LAST_KIND, "filterBar.last.kind");
    assert_eq!(keys::filter_bar::LAST_ORIENT, "filterBar.last.orient");
    assert_eq!(keys::filter_bar::LAST_RESOLUTION, "filterBar.last.resolution");
    assert_eq!(keys::filter_bar::LAST_SORT, "filterBar.last.sort");
    assert_eq!(keys::filter_bar::OFFSET_X, "filterBar.offsetX");
    assert_eq!(keys::filter_bar::OFFSET_Y, "filterBar.offsetY");
    assert_eq!(keys::filter_bar::ORIENTATION, "filterBar.orientation");
    assert_eq!(keys::filter_bar::RESOLUTION_PRESETS, "filterBar.resolutionPresets");
    assert_eq!(keys::filter_bar::SHOW_COLORS, "filterBar.show.colors");
    assert_eq!(keys::filter_bar::SHOW_FAVOURITES, "filterBar.show.favourites");
    assert_eq!(keys::filter_bar::SHOW_FOLDER, "filterBar.show.folder");
    assert_eq!(keys::filter_bar::SHOW_RANDOM, "filterBar.show.random");
    assert_eq!(keys::filter_bar::SHOW_RESOLUTION, "filterBar.show.resolution");
    assert_eq!(keys::filter_bar::SHOW_TAG_CLOUD, "filterBar.show.tagcloud");
    assert_eq!(keys::filter_bar::SHOW_TAGGING, "filterBar.show.tagging");
    assert_eq!(keys::filter_bar::SHOW_THEME, "filterBar.show.theme");
    assert_eq!(keys::filter_bar::STICKY, "filterBar.sticky");
    assert_eq!(keys::filter_bar::VISUAL_STYLE, "filterBar.visualStyle");
    assert_eq!(keys::general::CLOSE_ON_SELECTION, "general.closeOnSelection");
    assert_eq!(keys::general::FILTER_BAR_ALWAYS_VISIBLE, "general.filterBarAlwaysVisible");
    assert_eq!(keys::general::LOCALE, "general.locale");
    assert_eq!(keys::general::MAX_FPS, "general.maxFps");
    assert_eq!(keys::general::NOTIFY_ON_WALLPAPER_CHANGE, "general.notifyOnWallpaperChange");
    assert_eq!(keys::general::OPEN_FADE_FROM, "general.openFadeFrom");
    assert_eq!(keys::general::RANDOM_INCLUDE_FAVOURITES, "general.randomIncludeFavourites");
    assert_eq!(keys::general::RANDOM_INCLUDE_STATIC, "general.randomIncludeStatic");
    assert_eq!(keys::general::RANDOM_INCLUDE_VIDEO, "general.randomIncludeVideo");
    assert_eq!(keys::general::RANDOM_INCLUDE_WE, "general.randomIncludeWE");
    assert_eq!(keys::general::RANDOM_INTERVAL, "general.randomInterval");
    assert_eq!(keys::general::RANDOM_ROTATE, "general.randomRotate");
    assert_eq!(keys::general::UI_SCALE, "general.uiScale");
    assert_eq!(keys::general::WEATHER_MATCH, "general.weatherMatch");
    assert_eq!(keys::history::DEPTH, "history.depth");
    assert_eq!(keys::history::ENABLED, "history.enabled");
    assert_eq!(keys::library::POLLING_FALLBACK, "library.pollingFallback");
    assert_eq!(keys::library::POLLING_INTERVAL_SECONDS, "library.pollingIntervalSeconds");
    assert_eq!(keys::matugen::COLOR_INDEX, "matugen.colorIndex");
    assert_eq!(keys::matugen::CONTRAST, "matugen.contrast");
    assert_eq!(keys::matugen::MODE, "matugen.mode");
    assert_eq!(keys::matugen::SCHEME_TYPE, "matugen.schemeType");
    assert_eq!(keys::niri::BACKDROP, "niri.backdrop");
    assert_eq!(keys::niri::BACKDROP_AUTO_THEME, "niri.backdropAutoTheme");
    assert_eq!(keys::niri::BACKDROP_DIM, "niri.backdropDim");
    assert_eq!(keys::niri::BACKDROP_FOLLOW_WALLPAPER, "niri.backdropFollowWallpaper");
    assert_eq!(keys::niri::BACKDROP_THEME, "niri.backdropTheme");
    assert_eq!(keys::niri::OVERVIEW_BACKDROP, "niri.overviewBackdrop");
    assert_eq!(keys::niri::OVERVIEW_BACKDROP_BLUR, "niri.overviewBackdropBlur");
    assert_eq!(keys::niri::OVERVIEW_BACKDROP_BLUR_ENABLED, "niri.overviewBackdropBlurEnabled");
    assert_eq!(keys::noctalia::HOVER_PREVIEW, "noctalia.hoverPreview");
    assert_eq!(keys::paper::AWWW_INVERT_Y, "paper.awww.invertY");
    assert_eq!(keys::paper::AWWW_TRANSITION_ANGLE, "paper.awww.transitionAngle");
    assert_eq!(keys::paper::AWWW_TRANSITION_BEZIER, "paper.awww.transitionBezier");
    assert_eq!(keys::paper::AWWW_TRANSITION_DURATION_MS, "paper.awww.transitionDurationMs");
    assert_eq!(keys::paper::AWWW_TRANSITION_FPS, "paper.awww.transitionFps");
    assert_eq!(keys::paper::AWWW_TRANSITION_POS, "paper.awww.transitionPos");
    assert_eq!(keys::paper::AWWW_TRANSITION_STEP, "paper.awww.transitionStep");
    assert_eq!(keys::paper::AWWW_TRANSITION_TYPE, "paper.awww.transitionType");
    assert_eq!(keys::paper::AWWW_TRANSITION_WAVE_HEIGHT, "paper.awww.transitionWaveHeight");
    assert_eq!(keys::paper::AWWW_TRANSITION_WAVE_WIDTH, "paper.awww.transitionWaveWidth");
    assert_eq!(keys::paper::AWWW_FILTER, "paper.awww.filter");
    assert_eq!(keys::paper::ENGINE, "paper.engine");
    assert_eq!(keys::paper::IDLE_PAUSE_SECONDS, "paper.idlePauseSeconds");
    assert_eq!(keys::paper::PERFORMANCE_MODE, "paper.performanceMode");
    assert_eq!(keys::paper::VIDEO_MULTI_PROCESS, "paper.videoMultiProcess");
    assert_eq!(keys::paper::VIDEO_ENGINE, "paper.videoEngine");
    assert_eq!(keys::paths::PAPER_BIN, "paths.paperBin");
    assert_eq!(keys::paths::PAPER_STILL_BIN, "paths.paperStillBin");
    assert_eq!(keys::paths::PAPER_VK_BIN, "paths.paperVkBin");
    assert_eq!(keys::paths::STEAM_WE_ASSETS, "paths.steamWeAssets");
    assert_eq!(keys::paths::DMS_BIN, "paths.dmsBin");
    assert_eq!(keys::paths::CAELESTIA_BIN, "paths.caelestiaBin");
    assert_eq!(keys::paths::NOCTALIA_BIN, "paths.noctaliaBin");
    assert_eq!(keys::plasma::LOCK_SCREEN_MODE, "plasma.lockScreen.mode");
    assert_eq!(keys::plasma::LOCK_SCREEN_IMAGE, "plasma.lockScreen.image");
    assert_eq!(keys::plasma::LOCK_SCREEN_DYNAMIC, "plasma.lockScreen.dynamic");
    assert_eq!(keys::paths::CACHE, "paths.cache");
    assert_eq!(keys::paths::STEAM, "paths.steam");
    assert_eq!(keys::paths::STEAM_WORKSHOP, "paths.steamWorkshop");
    assert_eq!(keys::paths::VIDEO_WALLPAPER, "paths.videoWallpaper");
    assert_eq!(keys::paths::WALLPAPER, "paths.wallpaper");
    assert_eq!(keys::paths::TEMPLATES, "paths.templates");
    assert_eq!(keys::playlist::ASSIGN, "playlist.assign");
    assert_eq!(keys::playlist::LISTS, "playlist.lists");
    assert_eq!(keys::schedule::LATITUDE, "schedule.latitude");
    assert_eq!(keys::schedule::LONGITUDE, "schedule.longitude");
    assert_eq!(keys::schedule::APPLY_ON_START, "schedule.applyOnStart");
    assert_eq!(keys::schedule::DAY_MODE, "schedule.dayMode");
    assert_eq!(keys::schedule::DAY_SET, "schedule.daySet");
    assert_eq!(keys::schedule::DAY_TIME, "schedule.dayTime");
    assert_eq!(keys::schedule::ENABLED, "schedule.enabled");
    assert_eq!(keys::schedule::ENTRIES, "schedule.entries");
    assert_eq!(keys::schedule::MIGRATED, "schedule.migrated");
    assert_eq!(keys::schedule::NIGHT_MODE, "schedule.nightMode");
    assert_eq!(keys::schedule::NIGHT_SET, "schedule.nightSet");
    assert_eq!(keys::schedule::NIGHT_TIME, "schedule.nightTime");
    assert_eq!(keys::schedule::RULES, "schedule.rules");
    assert_eq!(keys::schedule::TRIGGER, "schedule.trigger");
    assert_eq!(keys::tagging::DEFAULT_SEARCH_MODE, "tagging.defaultSearchMode");
    assert_eq!(keys::semantic::INDEX_PROFILE, "semantic.indexProfile");
    assert_eq!(keys::semantic::MANIFEST, "semantic.manifest");
    assert_eq!(keys::semantic::MODELS, "semantic.models");
    assert_eq!(keys::selector::EXPANDED_WIDTH, "components.wallpaperSelector.expandedWidth");
    assert_eq!(keys::selector::GRID_COLUMNS, "components.wallpaperSelector.gridColumns");
    assert_eq!(keys::selector::GRID_BORDER_WIDTH, "components.wallpaperSelector.gridBorderWidth");
    assert_eq!(keys::selector::GRID_CYLINDER_BEND, "components.wallpaperSelector.gridCylinderBend");
    assert_eq!(
        keys::selector::GRID_CYLINDER_RADIUS,
        "components.wallpaperSelector.gridCylinderRadius"
    );
    assert_eq!(keys::selector::GRID_CORNER_RADIUS, "components.wallpaperSelector.gridCornerRadius");
    assert_eq!(keys::selector::GRID_GAP_X, "components.wallpaperSelector.gridGapX");
    assert_eq!(keys::selector::GRID_GAP_Y, "components.wallpaperSelector.gridGapY");
    assert_eq!(keys::selector::GRID_LAYOUT, "components.wallpaperSelector.gridLayout");
    assert_eq!(
        keys::selector::GRID_SELECTED_SCALE,
        "components.wallpaperSelector.gridSelectedScale"
    );
    assert_eq!(
        keys::selector::GRID_STAGE_PERSPECTIVE,
        "components.wallpaperSelector.gridStagePerspective"
    );
    assert_eq!(
        keys::selector::GRID_STAGE_ROTATION,
        "components.wallpaperSelector.gridStageRotation"
    );
    assert_eq!(keys::selector::GRID_STAGE_SCALE, "components.wallpaperSelector.gridStageScale");
    assert_eq!(keys::selector::GRID_STAGE_X, "components.wallpaperSelector.gridStageX");
    assert_eq!(keys::selector::GRID_STAGE_Y, "components.wallpaperSelector.gridStageY");
    assert_eq!(
        keys::selector::GRID_FLOW_FREQUENCY,
        "components.wallpaperSelector.gridFlowFrequency"
    );
    assert_eq!(keys::selector::GRID_FLOW_WAVE, "components.wallpaperSelector.gridFlowWave");
    assert_eq!(
        keys::selector::GRID_SCALE_VARIANCE,
        "components.wallpaperSelector.gridScaleVariance"
    );
    assert_eq!(keys::selector::GRID_SCATTER, "components.wallpaperSelector.gridScatter");
    assert_eq!(
        keys::selector::GRID_STAGE_DEPTH_ANGLE,
        "components.wallpaperSelector.gridStageDepthAngle"
    );
    assert_eq!(keys::selector::GRID_STAGE_SHEAR_X, "components.wallpaperSelector.gridStageShearX");
    assert_eq!(keys::selector::GRID_STAGE_SHEAR_Y, "components.wallpaperSelector.gridStageShearY");
    assert_eq!(keys::selector::GRID_STAGGER, "components.wallpaperSelector.gridStagger");
    assert_eq!(keys::selector::GRID_ROUND_CORNERS, "components.wallpaperSelector.gridRoundCorners");
    assert_eq!(keys::selector::GRID_ROWS, "components.wallpaperSelector.gridRows");
    assert_eq!(keys::selector::GRID_THUMB_HEIGHT, "components.wallpaperSelector.gridThumbHeight");
    assert_eq!(keys::selector::GRID_THUMB_WIDTH, "components.wallpaperSelector.gridThumbWidth");
    assert_eq!(
        keys::selector::HEX_ARC_INTENSITY_X10,
        "components.wallpaperSelector.hexArcIntensityX10"
    );
    assert_eq!(keys::selector::HEX_COLS, "components.wallpaperSelector.hexCols");
    assert_eq!(keys::selector::HEX_RADIUS, "components.wallpaperSelector.hexRadius");
    assert_eq!(keys::selector::HEX_ROWS, "components.wallpaperSelector.hexRows");
    assert_eq!(keys::selector::HEX_SCROLL_STEP, "components.wallpaperSelector.hexScrollStep");
    assert_eq!(keys::selector::HEX_SHAPE, "components.wallpaperSelector.hexShape");
    assert_eq!(keys::selector::HEX_ASPECT, "components.wallpaperSelector.hexAspect");
    assert_eq!(keys::selector::HEX_CURVE, "components.wallpaperSelector.hexCurve");
    assert_eq!(
        keys::selector::HEX_CURVE_FREQUENCY,
        "components.wallpaperSelector.hexCurveFrequency"
    );
    assert_eq!(keys::selector::HEX_GAP_X, "components.wallpaperSelector.hexGapX");
    assert_eq!(keys::selector::HEX_GAP_Y, "components.wallpaperSelector.hexGapY");
    assert_eq!(keys::selector::HEX_LENS, "components.wallpaperSelector.hexLens");
    assert_eq!(keys::selector::HEX_LENS_RADIUS, "components.wallpaperSelector.hexLensRadius");
    assert_eq!(keys::selector::HEX_ORBIT, "components.wallpaperSelector.hexOrbit");
    assert_eq!(keys::selector::HEX_ORBIT_RADIUS, "components.wallpaperSelector.hexOrbitRadius");
    assert_eq!(keys::selector::HEX_SCATTER, "components.wallpaperSelector.hexScatter");
    assert_eq!(
        keys::selector::HEX_STAGE_DEPTH_ANGLE,
        "components.wallpaperSelector.hexStageDepthAngle"
    );
    assert_eq!(keys::selector::HEX_STAGE_SHEAR_X, "components.wallpaperSelector.hexStageShearX");
    assert_eq!(keys::selector::HEX_STAGE_SHEAR_Y, "components.wallpaperSelector.hexStageShearY");
    assert_eq!(keys::selector::HEX_TWIST, "components.wallpaperSelector.hexTwist");
    assert_eq!(
        keys::selector::HEX_STAGE_PERSPECTIVE,
        "components.wallpaperSelector.hexStagePerspective"
    );
    assert_eq!(keys::selector::HEX_STAGE_ROTATION, "components.wallpaperSelector.hexStageRotation");
    assert_eq!(keys::selector::HEX_STAGE_SCALE, "components.wallpaperSelector.hexStageScale");
    assert_eq!(keys::selector::HEX_STAGE_X, "components.wallpaperSelector.hexStageX");
    assert_eq!(keys::selector::HEX_STAGE_Y, "components.wallpaperSelector.hexStageY");
    assert_eq!(keys::selector::HEX_STAGGER, "components.wallpaperSelector.hexStagger");
    assert_eq!(keys::selector::SANDY_ARC, "components.wallpaperSelector.sandyArc");
    assert_eq!(keys::selector::SANDY_BLEND, "components.wallpaperSelector.sandyBlend");
    assert_eq!(keys::selector::SANDY_CENTER, "components.wallpaperSelector.sandyCenter");
    assert_eq!(keys::selector::SANDY_DURATION, "components.wallpaperSelector.sandyDuration");
    assert_eq!(keys::selector::SANDY_EDGE_SPEED, "components.wallpaperSelector.sandyEdgeSpeed");
    assert_eq!(keys::selector::SANDY_FAN, "components.wallpaperSelector.sandyFan");
    assert_eq!(keys::selector::SANDY_FRONT, "components.wallpaperSelector.sandyFront");
    assert_eq!(keys::selector::SANDY_GRAIN, "components.wallpaperSelector.sandyGrain");
    assert_eq!(keys::selector::SANDY_RES_SCALE, "components.wallpaperSelector.sandyResScale");
    assert_eq!(keys::selector::SANDY_LOD, "components.wallpaperSelector.sandyLod");
    assert_eq!(keys::selector::SANDY_LOD_AUTO, "components.wallpaperSelector.sandyLodAuto");
    assert_eq!(keys::selector::SANDY_ORBIT, "components.wallpaperSelector.sandyOrbit");
    assert_eq!(keys::selector::SANDY_RING_BLEND, "components.wallpaperSelector.sandyRingBlend");
    assert_eq!(keys::selector::SANDY_RING_HOLD, "components.wallpaperSelector.sandyRingHold");
    assert_eq!(keys::selector::SANDY_RING_SIZE, "components.wallpaperSelector.sandyRingSize");
    assert_eq!(keys::selector::SANDY_RING_SOFT, "components.wallpaperSelector.sandyRingSoft");
    assert_eq!(keys::selector::SANDY_RING_SPIN, "components.wallpaperSelector.sandyRingSpin");
    assert_eq!(keys::selector::SANDY_RING_WAVE, "components.wallpaperSelector.sandyRingWave");
    assert_eq!(keys::selector::SANDY_STAGE_X, "components.wallpaperSelector.sandyStageX");
    assert_eq!(keys::selector::SANDY_STAGE_Y, "components.wallpaperSelector.sandyStageY");
    assert_eq!(keys::selector::SANDY_SKEW, "components.wallpaperSelector.sandySkew");
    assert_eq!(keys::selector::SANDY_SLICE_HEIGHT, "components.wallpaperSelector.sandySliceHeight");
    assert_eq!(keys::selector::SANDY_SLICE_WIDTH, "components.wallpaperSelector.sandySliceWidth");
    assert_eq!(keys::selector::SANDY_SPACING, "components.wallpaperSelector.sandySpacing");
    assert_eq!(keys::selector::SANDY_STRANDS, "components.wallpaperSelector.sandyStrands");
    assert_eq!(keys::selector::SANDY_TURBULENCE, "components.wallpaperSelector.sandyTurbulence");
    assert_eq!(keys::selector::SANDY_TWIST, "components.wallpaperSelector.sandyTwist");
    assert_eq!(keys::selector::SANDY_WAIST, "components.wallpaperSelector.sandyWaist");
    assert_eq!(keys::selector::SKEW_OFFSET, "components.wallpaperSelector.skewOffset");
    assert_eq!(keys::selector::SLICE_HEIGHT, "components.wallpaperSelector.sliceHeight");
    assert_eq!(keys::selector::SLICE_STAGE_X, "components.wallpaperSelector.sliceStageX");
    assert_eq!(keys::selector::SLICE_STAGE_Y, "components.wallpaperSelector.sliceStageY");
    assert_eq!(keys::selector::SLICE_SPACING, "components.wallpaperSelector.sliceSpacing");
    assert_eq!(
        keys::selector::SLICE_WOBBLE_STRENGTH,
        "components.wallpaperSelector.sliceWobbleStrength"
    );
    assert_eq!(keys::selector::STEAM_COLUMNS, "components.wallpaperSelector.steamColumns");
    assert_eq!(keys::selector::STEAM_ROWS, "components.wallpaperSelector.steamRows");
    assert_eq!(keys::selector::STEAM_THUMB_HEIGHT, "components.wallpaperSelector.steamThumbHeight");
    assert_eq!(keys::selector::STEAM_THUMB_WIDTH, "components.wallpaperSelector.steamThumbWidth");
    assert_eq!(
        keys::selector::DOWNLOADER_WALL_COLUMNS,
        "components.wallpaperSelector.downloaderWallColumns"
    );
    assert_eq!(
        keys::selector::DOWNLOADER_WALL_ROWS,
        "components.wallpaperSelector.downloaderWallRows"
    );
    assert_eq!(
        keys::selector::DOWNLOADER_WALL_THUMB_HEIGHT,
        "components.wallpaperSelector.downloaderWallThumbHeight"
    );
    assert_eq!(
        keys::selector::DOWNLOADER_WALL_THUMB_WIDTH,
        "components.wallpaperSelector.downloaderWallThumbWidth"
    );
    assert_eq!(
        keys::selector::DOWNLOADER_WALL_GAP_X,
        "components.wallpaperSelector.downloaderWallGapX"
    );
    assert_eq!(
        keys::selector::DOWNLOADER_WALL_GAP_Y,
        "components.wallpaperSelector.downloaderWallGapY"
    );
    assert_eq!(
        keys::selector::DOWNLOADER_WALL_CORNER_RADIUS,
        "components.wallpaperSelector.downloaderWallCornerRadius"
    );
    assert_eq!(
        keys::selector::DOWNLOADER_WALL_BORDER_WIDTH,
        "components.wallpaperSelector.downloaderWallBorderWidth"
    );
    assert_eq!(keys::selector::TAG_CLOUD_HEIGHT, "components.wallpaperSelector.tagCloudHeight");
    assert_eq!(keys::selector::TAG_CLOUD_OFFSET_X, "components.wallpaperSelector.tagCloudOffsetX");
    assert_eq!(keys::selector::TAG_CLOUD_OFFSET_Y, "components.wallpaperSelector.tagCloudOffsetY");
    for (actual, expected) in [
        (
            keys::selector::SLICE_FILTER_BAR_OFFSET_X,
            "components.wallpaperSelector.sliceFilterBarOffsetX",
        ),
        (
            keys::selector::SLICE_FILTER_BAR_OFFSET_Y,
            "components.wallpaperSelector.sliceFilterBarOffsetY",
        ),
        (
            keys::selector::SLICE_SEARCH_PANEL_OFFSET_X,
            "components.wallpaperSelector.sliceSearchPanelOffsetX",
        ),
        (
            keys::selector::SLICE_SEARCH_PANEL_OFFSET_Y,
            "components.wallpaperSelector.sliceSearchPanelOffsetY",
        ),
        (
            keys::selector::HEX_FILTER_BAR_OFFSET_X,
            "components.wallpaperSelector.hexFilterBarOffsetX",
        ),
        (
            keys::selector::HEX_FILTER_BAR_OFFSET_Y,
            "components.wallpaperSelector.hexFilterBarOffsetY",
        ),
        (
            keys::selector::HEX_SEARCH_PANEL_OFFSET_X,
            "components.wallpaperSelector.hexSearchPanelOffsetX",
        ),
        (
            keys::selector::HEX_SEARCH_PANEL_OFFSET_Y,
            "components.wallpaperSelector.hexSearchPanelOffsetY",
        ),
        (
            keys::selector::GRID_FILTER_BAR_OFFSET_X,
            "components.wallpaperSelector.gridFilterBarOffsetX",
        ),
        (
            keys::selector::GRID_FILTER_BAR_OFFSET_Y,
            "components.wallpaperSelector.gridFilterBarOffsetY",
        ),
        (
            keys::selector::GRID_SEARCH_PANEL_OFFSET_X,
            "components.wallpaperSelector.gridSearchPanelOffsetX",
        ),
        (
            keys::selector::GRID_SEARCH_PANEL_OFFSET_Y,
            "components.wallpaperSelector.gridSearchPanelOffsetY",
        ),
        (
            keys::selector::SANDY_FILTER_BAR_OFFSET_X,
            "components.wallpaperSelector.sandyFilterBarOffsetX",
        ),
        (
            keys::selector::SANDY_FILTER_BAR_OFFSET_Y,
            "components.wallpaperSelector.sandyFilterBarOffsetY",
        ),
        (
            keys::selector::SANDY_SEARCH_PANEL_OFFSET_X,
            "components.wallpaperSelector.sandySearchPanelOffsetX",
        ),
        (
            keys::selector::SANDY_SEARCH_PANEL_OFFSET_Y,
            "components.wallpaperSelector.sandySearchPanelOffsetY",
        ),
    ] {
        assert_eq!(actual, expected);
    }
    assert_eq!(keys::selector::TAG_CLOUD_ROWS, "components.wallpaperSelector.tagCloudRows");
    assert_eq!(keys::selector::VISIBLE_COUNT, "components.wallpaperSelector.visibleCount");
    assert_eq!(keys::selector::WALLHAVEN_COLUMNS, "components.wallpaperSelector.wallhavenColumns");
    assert_eq!(keys::selector::WALLHAVEN_ROWS, "components.wallpaperSelector.wallhavenRows");
    assert_eq!(
        keys::selector::WALLHAVEN_THUMB_WIDTH,
        "components.wallpaperSelector.wallhavenThumbWidth"
    );
    assert_eq!(keys::selector::LIVE_PREVIEW, "selector.livePreview");
    assert_eq!(keys::selector::SHOW_TYPE_BADGES, "components.wallpaperSelector.showTypeBadges");
    assert_eq!(keys::selector::PRESET_DRAFT_NAME, "components.wallpaperSelector.presetDraftName");
    assert_eq!(keys::selector::COMPONENTS, "components.wallpaperSelector");
    assert_eq!(keys::selector::CORNER_BL, "components.wallpaperSelector.cornerBL");
    assert_eq!(keys::selector::CORNER_BR, "components.wallpaperSelector.cornerBR");
    assert_eq!(keys::selector::CORNER_RADIUS, "components.wallpaperSelector.cornerRadius");
    assert_eq!(keys::selector::CORNER_TL, "components.wallpaperSelector.cornerTL");
    assert_eq!(keys::selector::CORNER_TR, "components.wallpaperSelector.cornerTR");
    assert_eq!(keys::selector::DISPLAY_MODE, "components.wallpaperSelector.displayMode");
    assert_eq!(keys::selector::ENABLED, "components.wallpaperSelector.enabled");
    assert_eq!(keys::selector::FLIP_BACK_REVEAL, "components.wallpaperSelector.flipBackReveal");
    assert_eq!(keys::selector::FLIP_DURATION_MS, "components.wallpaperSelector.flipDurationMs");
    assert_eq!(keys::selector::FLIP_EFFECT, "components.wallpaperSelector.flipEffect");
    assert_eq!(keys::selector::FLIP_SHADER, "components.wallpaperSelector.flipShader");
    assert_eq!(keys::selector::HEX_ARC, "components.wallpaperSelector.hexArc");
    assert_eq!(keys::selector::HEX_ARC_INTENSITY, "components.wallpaperSelector.hexArcIntensity");
    assert_eq!(keys::selector::ROUND_CORNERS, "components.wallpaperSelector.roundCorners");
    assert_eq!(
        keys::selector::SANDY_OUTGOING_LIVE,
        "components.wallpaperSelector.sandyOutgoingLive"
    );
    assert_eq!(keys::selector::SANDY_SWAP_LOOP, "components.wallpaperSelector.sandySwapLoop");
    assert_eq!(keys::selector::SANDY_SWAP_STYLE, "components.wallpaperSelector.sandySwapStyle");
    assert_eq!(keys::selector::SANDY_VORTEX, "components.wallpaperSelector.sandyVortex");
    assert_eq!(keys::selector::SLICE_WIDTH, "components.wallpaperSelector.sliceWidth");
    assert_eq!(keys::selector::SLICE_WOBBLE, "components.wallpaperSelector.sliceWobble");
    assert_eq!(keys::selector::TAG_CLOUD_WIDTH, "components.wallpaperSelector.tagCloudWidth");
    assert_eq!(keys::sources::YOUTUBE_MAX_HEIGHT, "sources.youtube.maxHeight");
    assert_eq!(keys::sources::BING_ENABLED, "sources.bing.enabled");
    assert_eq!(keys::sources::BING_MARKET, "sources.bing.market");
    assert_eq!(keys::sources::PEXELS_API_KEY, "sources.pexels.apiKey");
    assert_eq!(keys::sources::PEXELS_ENABLED, "sources.pexels.enabled");
    assert_eq!(keys::sources::UNSPLASH_ACCESS_KEY, "sources.unsplash.accessKey");
    assert_eq!(keys::sources::UNSPLASH_ENABLED, "sources.unsplash.enabled");
    assert_eq!(keys::sources::YOUTUBE_ENABLED, "sources.youtube.enabled");
    assert_eq!(keys::sources::YOUTUBE_MAX_MINUTES, "sources.youtube.maxMinutes");
    assert_eq!(keys::steam::API_KEY, "steam.apiKey");
    assert_eq!(keys::steam::BACKEND, "steam.backend");
    assert_eq!(keys::steam::USERNAME, "steam.username");
    assert_eq!(keys::theme::BACKEND, "theme.backend");
    assert_eq!(keys::theme::POLICY, "theme.policy");
    assert_eq!(keys::theme::AUTHORITY, "theme.authority");
    assert_eq!(keys::theme::TARGETS, "theme.targets");
    assert_eq!(keys::theme::ENGINE, "theme.engine");
    assert_eq!(keys::theme::CUSTOM_COLORS, "theme.customColors");
    assert_eq!(keys::theme::MODE, "theme.mode");
    assert_eq!(keys::theme::NATIVE_COLORS_PATH, "theme.nativeColorsPath");
    assert_eq!(keys::theme::NATIVE_TEMPLATES, "theme.nativeTemplates");
    assert_eq!(keys::theme::NOCTALIA_PURE_BLACK, "theme.noctaliaPureBlack");
    assert_eq!(keys::theme::NOCTALIA_SCHEME, "theme.noctaliaScheme");
    assert_eq!(keys::theme::PYWAL_SATURATE, "theme.pywalSaturate");
    assert_eq!(keys::theme::SAVED_THEMES, "theme.savedThemes");
    assert_eq!(keys::theme::STATIC_THEME, "theme.staticTheme");
    assert_eq!(keys::theme::WALLUST_COLORSPACE, "theme.wallustColorspace");
    assert_eq!(keys::theme::WALLUST_PALETTE, "theme.wallustPalette");
    assert_eq!(keys::transition::DURATION_MS, "transition.durationMs");
    assert_eq!(keys::transition::ENABLED, "transition.enabled");
    assert_eq!(keys::transition::PREVIEW, "transition.preview");
    assert_eq!(keys::transition::PREVIEW_FPS, "transition.previewFps");
    assert_eq!(keys::transition::SAND_FPS, "transition.sandFps");
    assert_eq!(keys::transition::SAND_PRIMARY, "transition.sandPrimary");
    assert_eq!(keys::transition::SAND_QUALITY, "transition.sandQuality");
    assert_eq!(keys::transition::SAND_SCOPE, "transition.sandScope");
    assert_eq!(keys::transition::SAND_SHARP, "transition.sandSharp");
    assert_eq!(keys::transition::SHADER, "transition.shader");
    assert_eq!(keys::transition::SHADER_SCOPES, "transition.shaderScopes");
    assert_eq!(keys::video_optimize::CODEC, "videoOptimize.codec");
    assert_eq!(keys::video_optimize::ENABLED, "videoOptimize.enabled");
    assert_eq!(keys::video_optimize::FIT_OUTPUTS, "videoOptimize.fitOutputs");
    assert_eq!(keys::video_optimize::HW_ENCODE, "videoOptimize.hwEncode");
    assert_eq!(keys::video_optimize::MAX_FPS, "videoOptimize.maxFps");
    assert_eq!(keys::video_optimize::MAX_HEIGHT, "videoOptimize.maxHeight");
    assert_eq!(keys::video_preview::DELAY_MS, "videoPreview.delayMs");
    assert_eq!(keys::video_preview::ENABLED, "videoPreview.enabled");
    assert_eq!(keys::video_preview::MODE, "videoPreview.mode");
    assert_eq!(keys::wallhaven::COLLECTIONS, "wallhaven.collections");
    assert_eq!(keys::wallhaven::API_KEY, "wallhaven.apiKey");
    assert_eq!(keys::wallhaven::USERNAME, "wallhaven.username");
    assert_eq!(keys::we_render::DISABLE_PARTICLES, "weRender.disableParticles");
    assert_eq!(keys::we_render::ENGINE, "weRender.engine");
    assert_eq!(keys::we_render::FPS, "weRender.fps");
    assert_eq!(keys::we_render::NATIVE, "weRender.native");
    assert_eq!(keys::we_render::SCALING, "weRender.scaling");
    assert_eq!(keys::workspace::DEBOUNCE_MS, "workspace.debounceMs");
    assert_eq!(keys::workspace::ENABLED, "workspace.enabled");
    assert_eq!(keys::workspace::SLIDE_MS, "workspace.slideMs");
    assert_eq!(keys::workspace::WALLPAPERS, "workspace.wallpapers");
    assert_eq!(keys::keybind::APPLY, "keys.apply");
    assert_eq!(keys::keybind::AUTOCOMPLETE, "keys.autocomplete");
    assert_eq!(keys::keybind::COLOR_NEXT, "keys.colorNext");
    assert_eq!(keys::keybind::COLOR_PREV, "keys.colorPrev");
    assert_eq!(keys::keybind::EFFECTS, "keys.effects");
    assert_eq!(keys::keybind::FAVOURITE, "keys.favourite");
    assert_eq!(keys::keybind::FILTER_BAR, "keys.filterBar");
    assert_eq!(keys::keybind::FLIP, "keys.flip");
    assert_eq!(keys::keybind::HELP, "keys.help");
    assert_eq!(keys::keybind::NAV_DOWN, "keys.navDown");
    assert_eq!(keys::keybind::NAV_LEFT, "keys.navLeft");
    assert_eq!(keys::keybind::NAV_RIGHT, "keys.navRight");
    assert_eq!(keys::keybind::NAV_UP, "keys.navUp");
    assert_eq!(keys::keybind::PLAYLISTS, "keys.playlists");
    assert_eq!(keys::keybind::SCENE_PROPERTIES, "keys.sceneProperties");
    assert_eq!(keys::keybind::SELECT, "keys.select");
    assert_eq!(keys::keybind::STUDIO, "keys.studio");
    assert_eq!(keys::keybind::SETTINGS, "keys.settings");
    assert_eq!(keys::keybind::TAG_CLOUD, "keys.tagCloud");
    assert_eq!(keys::keybind::TAG_MODE, "keys.tagMode");
    assert_eq!(keys::performance::AUTO_DELETE_IMAGE_TRASH, "performance.autoDeleteImageTrash");
    assert_eq!(keys::performance::AUTO_OPTIMIZE_IMAGES, "performance.autoOptimizeImages");
    assert_eq!(keys::performance::BATTERY_FPS, "performance.batteryFps");
    assert_eq!(keys::performance::BATTERY_SAVER, "performance.batterySaver");
    assert_eq!(
        keys::performance::BATTERY_VIDEO_IDLE_SECONDS,
        "performance.batteryVideoIdleSeconds"
    );
    assert_eq!(
        keys::performance::BATTERY_WALLPAPER_PERFORMANCE,
        "performance.batteryWallpaperPerformance"
    );
    assert_eq!(keys::performance::GPU_PREFERENCE, "performance.gpuPreference");
    assert_eq!(keys::performance::IMAGE_OPTIMIZE_PRESET, "performance.imageOptimizePreset");
    assert_eq!(keys::performance::IMAGE_OPTIMIZE_RESOLUTION, "performance.imageOptimizeResolution");
    assert_eq!(keys::performance::IMAGE_TRASH_DAYS, "performance.imageTrashDays");
    assert_eq!(keys::performance::MAX_THUMB_JOBS, "performance.maxThumbJobs");
}

#[test]
fn launch_keys_pinned() {
    assert_eq!(keys::launch::ANIMATION, "launch.animation");
    assert_eq!(keys::motion::FAST_MS, "motion.fastMs");
    assert_eq!(keys::motion::STANDARD_MS, "motion.standardMs");
    assert_eq!(keys::motion::SLOW_MS, "motion.slowMs");
    assert_eq!(keys::motion::LAUNCH_SPEED, "motion.launchSpeed");
    assert_eq!(keys::motion::FILTER_SWAP_SPEED, "motion.filterSwapSpeed");
}
