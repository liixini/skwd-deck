use std::marker::PhantomData;

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Boolean,
    Number,
    Text,
    Array,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DefaultValue {
    Boolean(bool),
    Number(f64),
    ResponsiveNumber { regular: f64, compact: f64 },
    Text(&'static str),
    EmptyArray,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumberBounds {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SettingSpec {
    pub path: &'static str,
    pub kind: ValueKind,
    pub default: DefaultValue,
    pub number_bounds: Option<NumberBounds>,
}

impl SettingSpec {
    pub const fn boolean(path: &'static str, default: bool) -> Self {
        Self {
            path,
            kind: ValueKind::Boolean,
            default: DefaultValue::Boolean(default),
            number_bounds: None,
        }
    }

    pub const fn number(path: &'static str, default: f64) -> Self {
        Self {
            path,
            kind: ValueKind::Number,
            default: DefaultValue::Number(default),
            number_bounds: None,
        }
    }

    pub const fn bounded_number(path: &'static str, default: f64, min: f64, max: f64) -> Self {
        Self {
            path,
            kind: ValueKind::Number,
            default: DefaultValue::Number(default),
            number_bounds: Some(NumberBounds { min, max }),
        }
    }

    pub const fn responsive_number(path: &'static str, regular: f64, compact: f64) -> Self {
        Self {
            path,
            kind: ValueKind::Number,
            default: DefaultValue::ResponsiveNumber { regular, compact },
            number_bounds: None,
        }
    }

    pub const fn text(path: &'static str, default: &'static str) -> Self {
        Self {
            path,
            kind: ValueKind::Text,
            default: DefaultValue::Text(default),
            number_bounds: None,
        }
    }

    pub const fn array(path: &'static str) -> Self {
        Self {
            path,
            kind: ValueKind::Array,
            default: DefaultValue::EmptyArray,
            number_bounds: None,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Setting<T> {
    spec: SettingSpec,
    marker: PhantomData<fn() -> T>,
}

impl<T> Clone for Setting<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Setting<T> {}

impl<T> Setting<T> {
    pub const fn path(self) -> &'static str {
        self.spec.path
    }

    pub const fn spec(self) -> SettingSpec {
        self.spec
    }
}

impl Setting<bool> {
    pub const fn boolean(path: &'static str, default: bool) -> Self {
        Self { spec: SettingSpec::boolean(path, default), marker: PhantomData }
    }

    pub fn read(self, root: &Value) -> bool {
        let DefaultValue::Boolean(default) = self.spec.default else { unreachable!() };
        crate::bool_at(root, self.path(), default)
    }

    pub const fn default(self) -> bool {
        let DefaultValue::Boolean(default) = self.spec.default else { unreachable!() };
        default
    }
}

impl Setting<f64> {
    pub const fn number(path: &'static str, default: f64) -> Self {
        Self { spec: SettingSpec::number(path, default), marker: PhantomData }
    }

    pub const fn bounded_number(path: &'static str, default: f64, min: f64, max: f64) -> Self {
        Self { spec: SettingSpec::bounded_number(path, default, min, max), marker: PhantomData }
    }

    pub const fn responsive_number(path: &'static str, regular: f64, compact: f64) -> Self {
        Self { spec: SettingSpec::responsive_number(path, regular, compact), marker: PhantomData }
    }

    pub fn read(self, root: &Value) -> f64 {
        self.read_with(root, false)
    }

    pub fn read_with(self, root: &Value, compact: bool) -> f64 {
        let default = match self.spec.default {
            DefaultValue::Number(default) => default,
            DefaultValue::ResponsiveNumber { regular, compact: compact_default } => {
                if compact {
                    compact_default
                } else {
                    regular
                }
            }
            _ => unreachable!(),
        };
        let value = crate::num_at(root, self.path(), default);
        self.spec.number_bounds.map_or(value, |bounds| value.clamp(bounds.min, bounds.max))
    }

    pub const fn default(self) -> f64 {
        match self.spec.default {
            DefaultValue::Number(default) => default,
            DefaultValue::ResponsiveNumber { regular, .. } => regular,
            _ => unreachable!(),
        }
    }
}

impl Setting<String> {
    pub const fn text(path: &'static str, default: &'static str) -> Self {
        Self { spec: SettingSpec::text(path, default), marker: PhantomData }
    }

    pub fn read(self, root: &Value) -> String {
        let DefaultValue::Text(default) = self.spec.default else { unreachable!() };
        crate::str_at(root, self.path(), default)
    }

    pub const fn default(self) -> &'static str {
        let DefaultValue::Text(default) = self.spec.default else { unreachable!() };
        default
    }
}

pub mod setting {
    use super::Setting;
    use crate::keys;

    pub mod filter_bar {
        use super::{Setting, keys};

        pub const DEFAULT_FOLDER: Setting<String> =
            Setting::text(keys::filter_bar::DEFAULT_FOLDER, "*");
        pub const OFFSET_X: Setting<f64> = Setting::number(keys::filter_bar::OFFSET_X, 0.0);
        pub const OFFSET_Y: Setting<f64> = Setting::number(keys::filter_bar::OFFSET_Y, 0.0);
        pub const ORIENTATION: Setting<String> =
            Setting::text(keys::filter_bar::ORIENTATION, "horizontal");
        pub const STICKY: Setting<bool> = Setting::boolean(keys::filter_bar::STICKY, false);
        pub const VISUAL_STYLE: Setting<String> =
            Setting::text(keys::filter_bar::VISUAL_STYLE, "match");
    }

    pub mod general {
        use super::{Setting, keys};

        pub const CLOSE_ON_SELECTION: Setting<bool> =
            Setting::boolean(keys::general::CLOSE_ON_SELECTION, false);
        pub const FILTER_BAR_ALWAYS_VISIBLE: Setting<bool> =
            Setting::boolean(keys::general::FILTER_BAR_ALWAYS_VISIBLE, true);
        pub const MAX_FPS: Setting<f64> =
            Setting::bounded_number(keys::general::MAX_FPS, 120.0, 1.0, 1_000.0);
        pub const OPEN_FADE_FROM: Setting<f64> =
            Setting::bounded_number(keys::general::OPEN_FADE_FROM, 0.0, 0.0, 100.0);
        pub const RANDOM_INTERVAL: Setting<f64> =
            Setting::number(keys::general::RANDOM_INTERVAL, 300.0);
        pub const SETTINGS_STYLE: Setting<String> =
            Setting::text(keys::general::SETTINGS_STYLE, "editorial");
        pub const UI_SCALE: Setting<f64> =
            Setting::bounded_number(keys::general::UI_SCALE, 1.0, 0.5, 2.0);
    }

    pub mod motion {
        use super::{Setting, keys};

        pub const FAST_MS: Setting<f64> =
            Setting::bounded_number(keys::motion::FAST_MS, 180.0, 35.0, 2_000.0);
        pub const STANDARD_MS: Setting<f64> =
            Setting::bounded_number(keys::motion::STANDARD_MS, 250.0, 35.0, 2_000.0);
        pub const SLOW_MS: Setting<f64> =
            Setting::bounded_number(keys::motion::SLOW_MS, 450.0, 35.0, 4_000.0);
    }

    pub mod selector {
        use super::{Setting, keys};

        pub const DISPLAY_MODE: Setting<String> =
            Setting::text(keys::selector::DISPLAY_MODE, "slices");
        pub const FLIP_DURATION_MS: Setting<f64> =
            Setting::bounded_number(keys::selector::FLIP_DURATION_MS, 1500.0, 100.0, 5_000.0);
        pub const GRID_BORDER_WIDTH: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_BORDER_WIDTH, 2.0, 0.0, 32.0);
        pub const GRID_CYLINDER_BEND: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_CYLINDER_BEND, 65.0, -100.0, 100.0);
        pub const GRID_CYLINDER_RADIUS: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_CYLINDER_RADIUS, 720.0, 150.0, 2_400.0);
        pub const GRID_FILTER_BAR_OFFSET_X: Setting<f64> =
            Setting::number(keys::selector::GRID_FILTER_BAR_OFFSET_X, 0.0);
        pub const GRID_FILTER_BAR_OFFSET_Y: Setting<f64> =
            Setting::number(keys::selector::GRID_FILTER_BAR_OFFSET_Y, 0.0);
        pub const GRID_CORNER_RADIUS: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_CORNER_RADIUS, 6.0, 0.0, 128.0);
        pub const GRID_GAP_X: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_GAP_X, 8.0, 0.0, 256.0);
        pub const GRID_GAP_Y: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_GAP_Y, 8.0, 0.0, 256.0);
        pub const GRID_LAYOUT: Setting<String> =
            Setting::text(keys::selector::GRID_LAYOUT, "uniform");
        pub const GRID_FLOW_FREQUENCY: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_FLOW_FREQUENCY, 1.0, 0.1, 8.0);
        pub const GRID_FLOW_WAVE: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_FLOW_WAVE, 0.0, -200.0, 200.0);
        pub const GRID_ROUND_CORNERS: Setting<bool> =
            Setting::boolean(keys::selector::GRID_ROUND_CORNERS, true);
        pub const GRID_SEARCH_PANEL_OFFSET_X: Setting<f64> =
            Setting::number(keys::selector::GRID_SEARCH_PANEL_OFFSET_X, 0.0);
        pub const GRID_SEARCH_PANEL_OFFSET_Y: Setting<f64> =
            Setting::number(keys::selector::GRID_SEARCH_PANEL_OFFSET_Y, 0.0);
        pub const GRID_SELECTED_SCALE: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_SELECTED_SCALE, 100.0, 100.0, 200.0);
        pub const GRID_SCALE_VARIANCE: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_SCALE_VARIANCE, 0.0, 0.0, 75.0);
        pub const GRID_SCATTER: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_SCATTER, 0.0, 0.0, 150.0);
        pub const GRID_STAGE_DEPTH_ANGLE: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGE_DEPTH_ANGLE, 0.0, -180.0, 180.0);
        pub const GRID_STAGE_PERSPECTIVE: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGE_PERSPECTIVE, 0.0, -80.0, 80.0);
        pub const GRID_STAGE_ROTATION: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGE_ROTATION, 0.0, -45.0, 45.0);
        pub const GRID_STAGE_SCALE: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGE_SCALE, 100.0, 25.0, 250.0);
        pub const GRID_STAGE_SHEAR_X: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGE_SHEAR_X, 0.0, -100.0, 100.0);
        pub const GRID_STAGE_SHEAR_Y: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGE_SHEAR_Y, 0.0, -100.0, 100.0);
        pub const GRID_STAGE_X: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGE_X, 0.0, -100.0, 100.0);
        pub const GRID_STAGE_Y: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGE_Y, 0.0, -100.0, 100.0);
        pub const GRID_STAGGER: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGGER, 50.0, -100.0, 100.0);
        pub const HEX_ASPECT: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_ASPECT, 100.0, 50.0, 200.0);
        pub const HEX_CURVE: Setting<String> = Setting::text(keys::selector::HEX_CURVE, "arc");
        pub const HEX_CURVE_FREQUENCY: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_CURVE_FREQUENCY, 1.0, 0.25, 6.0);
        pub const HEX_FILTER_BAR_OFFSET_X: Setting<f64> =
            Setting::number(keys::selector::HEX_FILTER_BAR_OFFSET_X, 0.0);
        pub const HEX_FILTER_BAR_OFFSET_Y: Setting<f64> =
            Setting::number(keys::selector::HEX_FILTER_BAR_OFFSET_Y, 0.0);
        pub const HEX_GAP_X: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_GAP_X, 6.0, -100.0, 256.0);
        pub const HEX_GAP_Y: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_GAP_Y, 6.0, -100.0, 256.0);
        pub const HEX_LENS: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_LENS, 0.0, -50.0, 150.0);
        pub const HEX_LENS_RADIUS: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_LENS_RADIUS, 620.0, 100.0, 2400.0);
        pub const HEX_ORBIT: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_ORBIT, 0.0, -100.0, 100.0);
        pub const HEX_ORBIT_RADIUS: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_ORBIT_RADIUS, 620.0, 100.0, 2400.0);
        pub const HEX_SCATTER: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_SCATTER, 0.0, 0.0, 160.0);
        pub const HEX_SEARCH_PANEL_OFFSET_X: Setting<f64> =
            Setting::number(keys::selector::HEX_SEARCH_PANEL_OFFSET_X, 0.0);
        pub const HEX_SEARCH_PANEL_OFFSET_Y: Setting<f64> =
            Setting::number(keys::selector::HEX_SEARCH_PANEL_OFFSET_Y, 0.0);
        pub const HEX_SHAPE: Setting<String> = Setting::text(keys::selector::HEX_SHAPE, "hexagon");
        pub const HEX_STAGE_DEPTH_ANGLE: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGE_DEPTH_ANGLE, 0.0, -180.0, 180.0);
        pub const HEX_STAGE_PERSPECTIVE: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGE_PERSPECTIVE, 0.0, -80.0, 80.0);
        pub const HEX_STAGE_ROTATION: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGE_ROTATION, 0.0, -45.0, 45.0);
        pub const HEX_STAGE_SCALE: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGE_SCALE, 100.0, 25.0, 250.0);
        pub const HEX_STAGE_SHEAR_X: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGE_SHEAR_X, 0.0, -100.0, 100.0);
        pub const HEX_STAGE_SHEAR_Y: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGE_SHEAR_Y, 0.0, -100.0, 100.0);
        pub const HEX_STAGE_X: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGE_X, 0.0, -100.0, 100.0);
        pub const HEX_STAGE_Y: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGE_Y, 0.0, -100.0, 100.0);
        pub const HEX_STAGGER: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGGER, 50.0, -100.0, 150.0);
        pub const HEX_TWIST: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_TWIST, 0.0, -180.0, 180.0);
        pub const SANDY_FILTER_BAR_OFFSET_X: Setting<f64> =
            Setting::number(keys::selector::SANDY_FILTER_BAR_OFFSET_X, 0.0);
        pub const SANDY_FILTER_BAR_OFFSET_Y: Setting<f64> =
            Setting::number(keys::selector::SANDY_FILTER_BAR_OFFSET_Y, 0.0);
        pub const SANDY_SEARCH_PANEL_OFFSET_X: Setting<f64> =
            Setting::number(keys::selector::SANDY_SEARCH_PANEL_OFFSET_X, 0.0);
        pub const SANDY_SEARCH_PANEL_OFFSET_Y: Setting<f64> =
            Setting::number(keys::selector::SANDY_SEARCH_PANEL_OFFSET_Y, 0.0);
        pub const SANDY_STAGE_X: Setting<f64> =
            Setting::bounded_number(keys::selector::SANDY_STAGE_X, 0.0, -100.0, 100.0);
        pub const SANDY_STAGE_Y: Setting<f64> =
            Setting::bounded_number(keys::selector::SANDY_STAGE_Y, 0.0, -100.0, 100.0);
        pub const SLICE_FILTER_BAR_OFFSET_X: Setting<f64> =
            Setting::number(keys::selector::SLICE_FILTER_BAR_OFFSET_X, 0.0);
        pub const SLICE_FILTER_BAR_OFFSET_Y: Setting<f64> =
            Setting::number(keys::selector::SLICE_FILTER_BAR_OFFSET_Y, 0.0);
        pub const SLICE_SEARCH_PANEL_OFFSET_X: Setting<f64> =
            Setting::number(keys::selector::SLICE_SEARCH_PANEL_OFFSET_X, 0.0);
        pub const SLICE_SEARCH_PANEL_OFFSET_Y: Setting<f64> =
            Setting::number(keys::selector::SLICE_SEARCH_PANEL_OFFSET_Y, 0.0);
        pub const SLICE_STAGE_X: Setting<f64> =
            Setting::bounded_number(keys::selector::SLICE_STAGE_X, 0.0, -100.0, 100.0);
        pub const SLICE_STAGE_Y: Setting<f64> =
            Setting::bounded_number(keys::selector::SLICE_STAGE_Y, 0.0, -100.0, 100.0);
    }

    pub mod transition {
        use super::{Setting, keys};

        pub const DURATION_MS: Setting<f64> =
            Setting::bounded_number(keys::transition::DURATION_MS, 600.0, 50.0, 10_000.0);
        pub const ENABLED: Setting<bool> = Setting::boolean(keys::transition::ENABLED, true);
        pub const PREVIEW_FPS: Setting<f64> =
            Setting::bounded_number(keys::transition::PREVIEW_FPS, 30.0, 1.0, 1_000.0);
        pub const SHADER: Setting<String> = Setting::text(keys::transition::SHADER, "random");
    }
}

const STATIC_SPECS: &[SettingSpec] = &[
    setting::filter_bar::DEFAULT_FOLDER.spec(),
    setting::filter_bar::OFFSET_X.spec(),
    setting::filter_bar::OFFSET_Y.spec(),
    setting::filter_bar::ORIENTATION.spec(),
    setting::filter_bar::STICKY.spec(),
    setting::filter_bar::VISUAL_STYLE.spec(),
    setting::general::CLOSE_ON_SELECTION.spec(),
    setting::general::FILTER_BAR_ALWAYS_VISIBLE.spec(),
    setting::general::MAX_FPS.spec(),
    setting::general::OPEN_FADE_FROM.spec(),
    setting::general::RANDOM_INTERVAL.spec(),
    setting::general::SETTINGS_STYLE.spec(),
    setting::general::UI_SCALE.spec(),
    setting::motion::FAST_MS.spec(),
    setting::motion::STANDARD_MS.spec(),
    setting::motion::SLOW_MS.spec(),
    setting::selector::DISPLAY_MODE.spec(),
    setting::selector::FLIP_DURATION_MS.spec(),
    setting::selector::GRID_BORDER_WIDTH.spec(),
    setting::selector::GRID_CYLINDER_BEND.spec(),
    setting::selector::GRID_CYLINDER_RADIUS.spec(),
    setting::selector::GRID_FILTER_BAR_OFFSET_X.spec(),
    setting::selector::GRID_FILTER_BAR_OFFSET_Y.spec(),
    setting::selector::GRID_CORNER_RADIUS.spec(),
    setting::selector::GRID_GAP_X.spec(),
    setting::selector::GRID_GAP_Y.spec(),
    setting::selector::GRID_LAYOUT.spec(),
    setting::selector::GRID_FLOW_FREQUENCY.spec(),
    setting::selector::GRID_FLOW_WAVE.spec(),
    setting::selector::GRID_ROUND_CORNERS.spec(),
    setting::selector::GRID_SEARCH_PANEL_OFFSET_X.spec(),
    setting::selector::GRID_SEARCH_PANEL_OFFSET_Y.spec(),
    setting::selector::GRID_SELECTED_SCALE.spec(),
    setting::selector::GRID_SCALE_VARIANCE.spec(),
    setting::selector::GRID_SCATTER.spec(),
    setting::selector::GRID_STAGE_DEPTH_ANGLE.spec(),
    setting::selector::GRID_STAGE_PERSPECTIVE.spec(),
    setting::selector::GRID_STAGE_ROTATION.spec(),
    setting::selector::GRID_STAGE_SCALE.spec(),
    setting::selector::GRID_STAGE_SHEAR_X.spec(),
    setting::selector::GRID_STAGE_SHEAR_Y.spec(),
    setting::selector::GRID_STAGE_X.spec(),
    setting::selector::GRID_STAGE_Y.spec(),
    setting::selector::GRID_STAGGER.spec(),
    setting::selector::HEX_ASPECT.spec(),
    setting::selector::HEX_CURVE.spec(),
    setting::selector::HEX_CURVE_FREQUENCY.spec(),
    setting::selector::HEX_FILTER_BAR_OFFSET_X.spec(),
    setting::selector::HEX_FILTER_BAR_OFFSET_Y.spec(),
    setting::selector::HEX_GAP_X.spec(),
    setting::selector::HEX_GAP_Y.spec(),
    setting::selector::HEX_LENS.spec(),
    setting::selector::HEX_LENS_RADIUS.spec(),
    setting::selector::HEX_ORBIT.spec(),
    setting::selector::HEX_ORBIT_RADIUS.spec(),
    setting::selector::HEX_SCATTER.spec(),
    setting::selector::HEX_SEARCH_PANEL_OFFSET_X.spec(),
    setting::selector::HEX_SEARCH_PANEL_OFFSET_Y.spec(),
    setting::selector::HEX_SHAPE.spec(),
    setting::selector::HEX_STAGE_DEPTH_ANGLE.spec(),
    setting::selector::HEX_STAGE_PERSPECTIVE.spec(),
    setting::selector::HEX_STAGE_ROTATION.spec(),
    setting::selector::HEX_STAGE_SCALE.spec(),
    setting::selector::HEX_STAGE_SHEAR_X.spec(),
    setting::selector::HEX_STAGE_SHEAR_Y.spec(),
    setting::selector::HEX_STAGE_X.spec(),
    setting::selector::HEX_STAGE_Y.spec(),
    setting::selector::HEX_STAGGER.spec(),
    setting::selector::HEX_TWIST.spec(),
    setting::selector::SANDY_FILTER_BAR_OFFSET_X.spec(),
    setting::selector::SANDY_FILTER_BAR_OFFSET_Y.spec(),
    setting::selector::SANDY_SEARCH_PANEL_OFFSET_X.spec(),
    setting::selector::SANDY_SEARCH_PANEL_OFFSET_Y.spec(),
    setting::selector::SANDY_STAGE_X.spec(),
    setting::selector::SANDY_STAGE_Y.spec(),
    setting::selector::SLICE_FILTER_BAR_OFFSET_X.spec(),
    setting::selector::SLICE_FILTER_BAR_OFFSET_Y.spec(),
    setting::selector::SLICE_SEARCH_PANEL_OFFSET_X.spec(),
    setting::selector::SLICE_SEARCH_PANEL_OFFSET_Y.spec(),
    setting::selector::SLICE_STAGE_X.spec(),
    setting::selector::SLICE_STAGE_Y.spec(),
    setting::transition::DURATION_MS.spec(),
    setting::transition::ENABLED.spec(),
    setting::transition::PREVIEW_FPS.spec(),
    setting::transition::SHADER.spec(),
    SettingSpec::boolean(crate::keys::selector::LIVE_PREVIEW, true),
    SettingSpec::boolean(crate::keys::selector::SHOW_TYPE_BADGES, true),
    SettingSpec::boolean(crate::keys::selector::HEX_ARC, true),
    SettingSpec::boolean(crate::keys::filter_bar::SHOW_COLORS, true),
    SettingSpec::boolean(crate::keys::filter_bar::SHOW_FAVOURITES, true),
    SettingSpec::boolean(crate::keys::filter_bar::SHOW_FOLDER, true),
    SettingSpec::boolean(crate::keys::filter_bar::SHOW_RANDOM, true),
    SettingSpec::boolean(crate::keys::filter_bar::SHOW_RESOLUTION, true),
    SettingSpec::boolean(crate::keys::filter_bar::SHOW_TAG_CLOUD, true),
    SettingSpec::boolean(crate::keys::filter_bar::SHOW_TAGGING, true),
    SettingSpec::boolean(crate::keys::filter_bar::SHOW_THEME, true),
    SettingSpec::boolean(crate::keys::selector::SANDY_LOD_AUTO, true),
    SettingSpec::boolean(crate::keys::selector::FLIP_SHADER, true),
    SettingSpec::boolean(crate::keys::selector::FLIP_BACK_REVEAL, true),
    SettingSpec::boolean(crate::keys::noctalia::HOVER_PREVIEW, true),
    SettingSpec::boolean(crate::keys::dms::HOVER_PREVIEW, true),
    SettingSpec::boolean(crate::keys::features::MATUGEN, true),
    SettingSpec::boolean(crate::keys::features::STEAM, true),
    SettingSpec::boolean(crate::keys::features::WALLHAVEN, true),
    SettingSpec::boolean(crate::keys::library::POLLING_FALLBACK, false),
    SettingSpec::boolean(crate::keys::wallpaper::MUTE, true),
    SettingSpec::boolean(crate::keys::niri::OVERVIEW_BACKDROP_BLUR_ENABLED, true),
    SettingSpec::boolean(crate::keys::niri::BACKDROP_FOLLOW_WALLPAPER, true),
    SettingSpec::boolean(crate::keys::general::RANDOM_INCLUDE_STATIC, true),
    SettingSpec::boolean(crate::keys::general::RANDOM_INCLUDE_VIDEO, true),
    SettingSpec::boolean(crate::keys::general::RANDOM_INCLUDE_WE, true),
    SettingSpec::boolean(crate::keys::general::RANDOM_INCLUDE_FAVOURITES, true),
    SettingSpec::boolean(crate::keys::general::NOTIFY_ON_WALLPAPER_CHANGE, true),
    SettingSpec::boolean(crate::keys::system::RESTORE_ON_STARTUP, true),
    SettingSpec::boolean(crate::keys::performance::BATTERY_SAVER, true),
    SettingSpec::boolean(crate::keys::effects::AUTO_RECOLOR, false),
    SettingSpec::boolean(crate::keys::general::RANDOM_ROTATE, false),
    SettingSpec::boolean(crate::keys::general::WEATHER_MATCH, false),
    SettingSpec::boolean(crate::keys::selector::ROUND_CORNERS, false),
    SettingSpec::boolean(crate::keys::selector::SLICE_WOBBLE, true),
    SettingSpec::boolean(crate::keys::paper::PERFORMANCE_MODE, false),
    SettingSpec::boolean(crate::keys::paper::VIDEO_MULTI_PROCESS, true),
    SettingSpec::boolean(crate::keys::performance::AUTO_DELETE_IMAGE_TRASH, false),
    SettingSpec::boolean(crate::keys::performance::AUTO_OPTIMIZE_IMAGES, false),
    SettingSpec::boolean(crate::keys::performance::BATTERY_WALLPAPER_PERFORMANCE, false),
    SettingSpec::boolean(crate::keys::system::PICK_ONLY_MODE, false),
    SettingSpec::boolean(crate::keys::system::POST_PROCESS_ON_RESTORE, false),
    SettingSpec::boolean(crate::keys::schedule::APPLY_ON_START, true),
    SettingSpec::boolean(crate::keys::schedule::ENABLED, false),
    SettingSpec::boolean(crate::keys::sources::BING_ENABLED, false),
    SettingSpec::boolean(crate::keys::sources::PEXELS_ENABLED, false),
    SettingSpec::boolean(crate::keys::sources::UNSPLASH_ENABLED, false),
    SettingSpec::boolean(crate::keys::sources::YOUTUBE_ENABLED, false),
    SettingSpec::boolean(crate::keys::transition::PREVIEW, true),
    SettingSpec::boolean(crate::keys::video_preview::ENABLED, true),
    SettingSpec::boolean(crate::keys::we_render::DISABLE_PARTICLES, false),
    SettingSpec::boolean(crate::keys::workspace::ENABLED, false),
    SettingSpec::boolean(crate::keys::niri::BACKDROP_AUTO_THEME, false),
    SettingSpec::boolean(crate::keys::niri::OVERVIEW_BACKDROP, false),
    SettingSpec::boolean(crate::keys::selector::SANDY_OUTGOING_LIVE, true),
    SettingSpec::boolean(crate::keys::selector::SANDY_SWAP_LOOP, false),
    SettingSpec::boolean(crate::keys::paper::AWWW_INVERT_Y, false),
    SettingSpec::boolean(crate::keys::transition::SAND_SHARP, false),
    SettingSpec::text(crate::keys::paper::ENGINE, "skwd-paper"),
    SettingSpec::text(crate::keys::display::FILL_MODE, "fill"),
    SettingSpec::text(crate::keys::matugen::SCHEME_TYPE, "scheme-fidelity"),
    SettingSpec::text(crate::keys::matugen::MODE, "dark"),
    SettingSpec::text(crate::keys::steam::BACKEND, "steam"),
    SettingSpec::text(crate::keys::we_render::SCALING, "default"),
    SettingSpec::text(crate::keys::paper::AWWW_TRANSITION_TYPE, "wipe"),
    SettingSpec::text(crate::keys::paper::AWWW_FILTER, "Lanczos3"),
    SettingSpec::text(crate::keys::paper::AWWW_TRANSITION_POS, "center"),
    SettingSpec::text(crate::keys::performance::IMAGE_OPTIMIZE_PRESET, "balanced"),
    SettingSpec::text(crate::keys::performance::IMAGE_OPTIMIZE_RESOLUTION, "2k"),
    SettingSpec::text(crate::keys::performance::GPU_PREFERENCE, "auto"),
    SettingSpec::text(crate::keys::effects::AUTO_THEME, "Catppuccin"),
    SettingSpec::text(
        crate::keys::system::EXTERNAL_MATUGEN_COMMAND,
        "matugen -c %config% image %path% -t %scheme% -m %mode% --source-color-index %index%",
    ),
    SettingSpec::text(crate::keys::system::MONITOR, ""),
    SettingSpec::text(crate::keys::general::LOCALE, ""),
    SettingSpec::text(crate::keys::launch::ANIMATION, "fade"),
    SettingSpec::text(crate::keys::motion::FILTER_SWAP_SPEED, "slow"),
    SettingSpec::text(crate::keys::motion::LAUNCH_SPEED, "standard"),
    SettingSpec::text(crate::keys::selector::FLIP_EFFECT, "Ignite"),
    SettingSpec::text(crate::keys::paper::VIDEO_ENGINE, "vulkan"),
    SettingSpec::text(crate::keys::tagging::DEFAULT_SEARCH_MODE, "tags"),
    SettingSpec::text(crate::keys::semantic::INDEX_PROFILE, "full"),
    SettingSpec::text(crate::keys::semantic::MANIFEST, ""),
    SettingSpec::text(crate::keys::theme::BACKEND, "skwd-iris"),
    SettingSpec::text(crate::keys::theme::POLICY, ""),
    SettingSpec::text(crate::keys::theme::AUTHORITY, ""),
    SettingSpec::text(crate::keys::theme::ENGINE, ""),
    SettingSpec::text(crate::keys::theme::SCHEME, "tonal-spot"),
    SettingSpec::text(crate::keys::theme::STYLE, "natural"),
    SettingSpec::text(crate::keys::transition::FAMILY, "random"),
    SettingSpec::text(crate::keys::sources::BING_MARKET, "en-US"),
    SettingSpec::text(crate::keys::sources::PEXELS_API_KEY, ""),
    SettingSpec::text(crate::keys::sources::UNSPLASH_ACCESS_KEY, ""),
    SettingSpec::text(crate::keys::steam::API_KEY, ""),
    SettingSpec::text(crate::keys::steam::USERNAME, ""),
    SettingSpec::text(crate::keys::wallhaven::API_KEY, ""),
    SettingSpec::text(crate::keys::wallhaven::USERNAME, ""),
    SettingSpec::text(crate::keys::schedule::LATITUDE, ""),
    SettingSpec::text(crate::keys::schedule::LONGITUDE, ""),
    SettingSpec::text(crate::keys::paths::DMS_BIN, ""),
    SettingSpec::text(crate::keys::paths::CAELESTIA_BIN, ""),
    SettingSpec::text(crate::keys::paths::NOCTALIA_BIN, ""),
    SettingSpec::text(crate::keys::paths::STEAM, ""),
    SettingSpec::text(crate::keys::paths::STEAM_WE_ASSETS, ""),
    SettingSpec::text(crate::keys::paths::STEAM_WORKSHOP, ""),
    SettingSpec::text(crate::keys::paths::VIDEO_WALLPAPER, ""),
    SettingSpec::text(crate::keys::paths::WALLPAPER, ""),
    SettingSpec::text(crate::keys::matugen::DEFAULT_CONFIG, ""),
    SettingSpec::text(crate::keys::niri::BACKDROP, ""),
    SettingSpec::text(crate::keys::niri::BACKDROP_THEME, ""),
    SettingSpec::text(crate::keys::plasma::LOCK_SCREEN_DYNAMIC, "poster"),
    SettingSpec::text(crate::keys::plasma::LOCK_SCREEN_IMAGE, ""),
    SettingSpec::text(crate::keys::plasma::LOCK_SCREEN_MODE, "off"),
    SettingSpec::text(crate::keys::selector::SANDY_SWAP_STYLE, "vortex"),
    SettingSpec::text(crate::keys::paper::AWWW_TRANSITION_BEZIER, ""),
    SettingSpec::text(crate::keys::theme::CUSTOM_COLORS, ""),
    SettingSpec::text(crate::keys::theme::STATIC_THEME, "nord"),
    SettingSpec::text(crate::keys::theme::WALLUST_PALETTE, ""),
    SettingSpec::text(crate::keys::transition::SAND_PRIMARY, ""),
    SettingSpec::text(crate::keys::transition::SAND_QUALITY, "auto"),
    SettingSpec::text(crate::keys::transition::SAND_SCOPE, "all"),
    SettingSpec::text(crate::keys::keybind::APPLY, ""),
    SettingSpec::text(crate::keys::keybind::AUTOCOMPLETE, ""),
    SettingSpec::text(crate::keys::keybind::COLOR_NEXT, ""),
    SettingSpec::text(crate::keys::keybind::COLOR_PREV, ""),
    SettingSpec::text(crate::keys::keybind::EFFECTS, ""),
    SettingSpec::text(crate::keys::keybind::FAVOURITE, ""),
    SettingSpec::text(crate::keys::keybind::FILTER_BAR, ""),
    SettingSpec::text(crate::keys::keybind::FLIP, ""),
    SettingSpec::text(crate::keys::keybind::HELP, ""),
    SettingSpec::text(crate::keys::keybind::NAV_DOWN, ""),
    SettingSpec::text(crate::keys::keybind::NAV_LEFT, ""),
    SettingSpec::text(crate::keys::keybind::NAV_RIGHT, ""),
    SettingSpec::text(crate::keys::keybind::NAV_UP, ""),
    SettingSpec::text(crate::keys::keybind::PLAYLISTS, ""),
    SettingSpec::text(crate::keys::keybind::SCENE_PROPERTIES, ""),
    SettingSpec::text(crate::keys::keybind::SELECT, ""),
    SettingSpec::text(crate::keys::keybind::STUDIO, ""),
    SettingSpec::text(crate::keys::keybind::SETTINGS, ""),
    SettingSpec::text(crate::keys::keybind::TAG_CLOUD, ""),
    SettingSpec::text(crate::keys::keybind::TAG_MODE, ""),
    SettingSpec::number(crate::keys::paper::AWWW_TRANSITION_DURATION_MS, 1000.0),
    SettingSpec::number(crate::keys::paper::AWWW_TRANSITION_FPS, 60.0),
    SettingSpec::number(crate::keys::paper::AWWW_TRANSITION_STEP, 90.0),
    SettingSpec::number(crate::keys::paper::AWWW_TRANSITION_ANGLE, 45.0),
    SettingSpec::number(crate::keys::paper::AWWW_TRANSITION_WAVE_WIDTH, 20.0),
    SettingSpec::number(crate::keys::paper::AWWW_TRANSITION_WAVE_HEIGHT, 20.0),
    SettingSpec::bounded_number(crate::keys::performance::IMAGE_TRASH_DAYS, 7.0, 0.0, 3650.0),
    SettingSpec::bounded_number(crate::keys::performance::MAX_THUMB_JOBS, 16.0, 1.0, 32.0),
    SettingSpec::bounded_number(crate::keys::library::POLLING_INTERVAL_SECONDS, 60.0, 15.0, 3600.0),
    SettingSpec::bounded_number(
        crate::keys::performance::BATTERY_FPS,
        crate::DEFAULT_BATTERY_FPS as f64,
        0.0,
        360.0,
    ),
    SettingSpec::number(
        crate::keys::performance::BATTERY_VIDEO_IDLE_SECONDS,
        crate::DEFAULT_BATTERY_VIDEO_IDLE_SECONDS as f64,
    ),
    SettingSpec::bounded_number(crate::keys::we_render::FPS, 30.0, 1.0, 240.0),
    SettingSpec::number(crate::keys::matugen::CONTRAST, 0.0),
    SettingSpec::bounded_number(crate::keys::wallpaper::VOLUME, 100.0, 0.0, 100.0),
    SettingSpec::bounded_number(crate::keys::matugen::COLOR_INDEX, 0.0, 0.0, 3.0),
    SettingSpec::responsive_number(crate::keys::selector::EXPANDED_WIDTH, 924.0, 600.0),
    SettingSpec::responsive_number(crate::keys::selector::SLICE_HEIGHT, 520.0, 360.0),
    SettingSpec::responsive_number(crate::keys::selector::SLICE_WIDTH, 135.0, 90.0),
    SettingSpec::responsive_number(crate::keys::selector::VISIBLE_COUNT, 12.0, 8.0),
    SettingSpec::responsive_number(crate::keys::selector::SKEW_OFFSET, 35.0, 25.0),
    SettingSpec::bounded_number(crate::keys::selector::SLICE_EDGE_TILT, 0.0, -240.0, 240.0),
    SettingSpec::number(crate::keys::selector::SLICE_SPACING, -30.0),
    SettingSpec::number(crate::keys::selector::SLICE_WOBBLE_STRENGTH, 100.0),
    SettingSpec::number(crate::keys::selector::CORNER_TL, 16.0),
    SettingSpec::number(crate::keys::selector::CORNER_TR, 16.0),
    SettingSpec::number(crate::keys::selector::CORNER_BR, 16.0),
    SettingSpec::number(crate::keys::selector::CORNER_BL, 16.0),
    SettingSpec::responsive_number(crate::keys::selector::TAG_CLOUD_WIDTH, 760.0, 540.0),
    SettingSpec::responsive_number(crate::keys::selector::TAG_CLOUD_HEIGHT, 168.0, 150.0),
    SettingSpec::text(crate::keys::selector::TAG_CLOUD_ROWS, "2"),
    SettingSpec::number(crate::keys::selector::TAG_CLOUD_OFFSET_X, 0.0),
    SettingSpec::number(crate::keys::selector::TAG_CLOUD_OFFSET_Y, 0.0),
    SettingSpec::number(crate::keys::selector::WALLHAVEN_COLUMNS, 6.0),
    SettingSpec::number(crate::keys::selector::WALLHAVEN_ROWS, 3.0),
    SettingSpec::number(crate::keys::selector::WALLHAVEN_THUMB_WIDTH, 300.0),
    SettingSpec::number(crate::keys::selector::WALLHAVEN_THUMB_HEIGHT, 169.0),
    SettingSpec::number(crate::keys::selector::STEAM_COLUMNS, 6.0),
    SettingSpec::number(crate::keys::selector::STEAM_ROWS, 3.0),
    SettingSpec::number(crate::keys::selector::STEAM_THUMB_WIDTH, 300.0),
    SettingSpec::number(crate::keys::selector::STEAM_THUMB_HEIGHT, 169.0),
    SettingSpec::responsive_number(crate::keys::selector::GRID_COLUMNS, 6.0, 4.0),
    SettingSpec::number(crate::keys::selector::GRID_ROWS, 3.0),
    SettingSpec::responsive_number(crate::keys::selector::GRID_THUMB_WIDTH, 300.0, 220.0),
    SettingSpec::responsive_number(crate::keys::selector::GRID_THUMB_HEIGHT, 169.0, 124.0),
    SettingSpec::responsive_number(crate::keys::selector::HEX_RADIUS, 140.0, 100.0),
    SettingSpec::number(crate::keys::selector::HEX_ROWS, 3.0),
    SettingSpec::responsive_number(crate::keys::selector::HEX_COLS, 7.0, 5.0),
    SettingSpec::number(crate::keys::selector::HEX_SCROLL_STEP, 1.0),
    SettingSpec::number(crate::keys::selector::HEX_ARC_INTENSITY_X10, 12.0),
    SettingSpec::number(crate::keys::selector::HEX_ARC_INTENSITY, 1.2),
    SettingSpec::responsive_number(crate::keys::selector::SANDY_CENTER, 440.0, 330.0),
    SettingSpec::responsive_number(crate::keys::selector::SANDY_SLICE_WIDTH, 96.0, 68.0),
    SettingSpec::responsive_number(crate::keys::selector::SANDY_SLICE_HEIGHT, 180.0, 130.0),
    SettingSpec::responsive_number(crate::keys::selector::SANDY_SKEW, 12.0, 8.0),
    SettingSpec::responsive_number(crate::keys::selector::SANDY_SPACING, 26.0, 20.0),
    SettingSpec::bounded_number(crate::keys::selector::SANDY_DURATION, 1250.0, 200.0, 6000.0),
    SettingSpec::bounded_number(crate::keys::selector::SANDY_BLEND, 700.0, 100.0, 4000.0),
    SettingSpec::responsive_number(crate::keys::selector::SANDY_STRANDS, 22.0, 18.0),
    SettingSpec::number(crate::keys::selector::SANDY_TWIST, 100.0),
    SettingSpec::number(crate::keys::selector::SANDY_ORBIT, 100.0),
    SettingSpec::number(crate::keys::selector::SANDY_TURBULENCE, 100.0),
    SettingSpec::number(crate::keys::selector::SANDY_WAIST, 100.0),
    SettingSpec::number(crate::keys::selector::SANDY_FRONT, 100.0),
    SettingSpec::number(crate::keys::selector::SANDY_ARC, 100.0),
    SettingSpec::number(crate::keys::selector::SANDY_EDGE_SPEED, 100.0),
    SettingSpec::number(crate::keys::selector::SANDY_FAN, 100.0),
    SettingSpec::number(crate::keys::selector::SANDY_RING_SPIN, 100.0),
    SettingSpec::number(crate::keys::selector::SANDY_RING_SIZE, 100.0),
    SettingSpec::number(crate::keys::selector::SANDY_RING_WAVE, 100.0),
    SettingSpec::number(crate::keys::selector::SANDY_RING_SOFT, 100.0),
    SettingSpec::number(crate::keys::selector::SANDY_RING_BLEND, 100.0),
    SettingSpec::number(crate::keys::selector::SANDY_RING_HOLD, 200.0),
    SettingSpec::bounded_number(crate::keys::selector::SANDY_GRAIN, 3.0, 1.0, 32.0),
    SettingSpec::bounded_number(crate::keys::selector::SANDY_RES_SCALE, 100.0, 25.0, 100.0),
    SettingSpec::bounded_number(crate::keys::selector::SANDY_LOD, 2.0, 1.0, 4.0),
    SettingSpec::number(crate::keys::paper::IDLE_PAUSE_SECONDS, 0.0),
    SettingSpec::number(crate::keys::video_preview::DELAY_MS, 250.0),
    SettingSpec::number(crate::keys::sources::YOUTUBE_MAX_HEIGHT, 2160.0),
    SettingSpec::number(crate::keys::sources::YOUTUBE_MAX_MINUTES, 3.0),
    SettingSpec::number(crate::keys::transition::SAND_FPS, 0.0),
    SettingSpec::bounded_number(crate::keys::niri::OVERVIEW_BACKDROP_BLUR, 20.0, 0.0, 200.0),
    SettingSpec::bounded_number(crate::keys::niri::BACKDROP_DIM, 0.0, 0.0, 100.0),
    SettingSpec::array(crate::keys::filter_bar::RESOLUTION_PRESETS),
    SettingSpec::array(crate::keys::integrations::LIST),
    SettingSpec::array(crate::keys::post_processing::LIST),
    SettingSpec::array(crate::keys::semantic::MODELS),
    SettingSpec::array(crate::keys::theme::TARGETS),
    SettingSpec::array(crate::keys::workspace::WALLPAPERS),
    SettingSpec::text(crate::keys::selector::PRESET_DRAFT_NAME, ""),
];

pub fn all() -> &'static [SettingSpec] {
    STATIC_SPECS
}

pub fn find(path: &str) -> Option<&'static SettingSpec> {
    STATIC_SPECS.iter().find(|spec| spec.path == path)
}

pub fn value_kind(path: &str) -> Option<ValueKind> {
    if path.starts_with("display.outputLocks.")
        || path.starts_with("filterBar.show.")
        || path.ends_with("hexArc")
        || path.ends_with(".livePreview")
    {
        return Some(ValueKind::Boolean);
    }
    if path.starts_with("display.fillModes.") || path.starts_with("transition.shaderScopes.") {
        return Some(ValueKind::Text);
    }
    if path.ends_with("displayMode") {
        return Some(ValueKind::Text);
    }
    if path.starts_with("filterBar.resolutionPresets.") {
        return if path.ends_with(".width") || path.ends_with(".height") {
            Some(ValueKind::Number)
        } else if path.ends_with(".label")
            || path.ends_with(".orientation")
            || path.ends_with(".from")
            || path.ends_with(".to")
        {
            Some(ValueKind::Text)
        } else {
            None
        };
    }
    if path.starts_with("postProcessing.") || path.starts_with("integrations.") {
        return if path.ends_with(".livePreview") {
            Some(ValueKind::Boolean)
        } else {
            Some(ValueKind::Text)
        };
    }
    find(path).map(|spec| spec.kind)
}

pub fn boolean_default(path: &str) -> Option<bool> {
    if path.starts_with("display.outputLocks.") {
        return Some(false);
    }
    if path.starts_with("filterBar.show.")
        || path.ends_with("hexArc")
        || path.ends_with(".livePreview")
    {
        return Some(true);
    }
    find(path).and_then(|spec| match spec.default {
        DefaultValue::Boolean(value) => Some(value),
        _ => None,
    })
}

pub fn read_boolean(root: &Value, path: &str) -> Option<bool> {
    boolean_default(path).map(|default| crate::bool_at(root, path, default))
}

pub fn number_default(path: &str) -> Option<f64> {
    find(path).and_then(|spec| match spec.default {
        DefaultValue::Number(value) => Some(value),
        DefaultValue::ResponsiveNumber { regular, .. } => Some(regular),
        _ => None,
    })
}

pub fn read_number(root: &Value, path: &str) -> Option<f64> {
    read_number_with(root, path, false)
}

pub fn read_number_with(root: &Value, path: &str, compact: bool) -> Option<f64> {
    let spec = find(path)?;
    let default = match spec.default {
        DefaultValue::Number(default) => default,
        DefaultValue::ResponsiveNumber { regular, compact: compact_default } => {
            if compact {
                compact_default
            } else {
                regular
            }
        }
        _ => return None,
    };
    let value = crate::num_at(root, path, default);
    Some(spec.number_bounds.map_or(value, |bounds| value.clamp(bounds.min, bounds.max)))
}

pub fn text_default(path: &str) -> Option<&'static str> {
    if path.ends_with("displayMode") {
        return Some("slices");
    }
    find(path).and_then(|spec| match spec.default {
        DefaultValue::Text(value) => Some(value),
        _ => None,
    })
}

pub fn read_text(root: &Value, path: &str) -> Option<String> {
    text_default(path).map(|default| crate::str_at(root, path, default))
}

pub fn normalize_value(path: &str, value: &Value) -> Option<Value> {
    let Some(kind) = value_kind(path) else {
        return Some(value.clone());
    };
    match kind {
        ValueKind::Boolean => value.as_bool().map(Value::Bool),
        ValueKind::Number => {
            let number = value.as_f64().or_else(|| value.as_str()?.trim().parse::<f64>().ok())?;
            if !number.is_finite() {
                return None;
            }
            let number = find(path)
                .and_then(|spec| spec.number_bounds)
                .map_or(number, |bounds| number.clamp(bounds.min, bounds.max));
            if number.fract() == 0.0 && number >= i64::MIN as f64 && number <= i64::MAX as f64 {
                Some(Value::Number((number as i64).into()))
            } else {
                serde_json::Number::from_f64(number).map(Value::Number)
            }
        }
        ValueKind::Text => match value {
            Value::String(text) => Some(Value::String(text.clone())),
            Value::Number(number) => Some(Value::String(number.to_string())),
            _ => None,
        },
        ValueKind::Array => value.as_array().cloned().map(Value::Array),
    }
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
