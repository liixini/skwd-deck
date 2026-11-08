use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, Weak};
use std::time::SystemTime;

use paper_control::{
    Depth, Labels, MAX_OUTLINES, MAX_WIDGETS, MONTHS, Palette, WEEKDAYS, Widget, WidgetScene,
};
use serde_json::{Value, json};
use skwd_config::keys::widgets as keys;
use skwd_palette::{Rgb, parse_hex};
use skwd_wall_core::config::Config;
use skwd_wall_core::infrastructure::drawing as cache;
use skwd_wall_core::infrastructure::renderers::WidgetSource;
use skwd_wall_core::{WallState, lock};
use wall_proto::widgets::{Desk, DeskOutput, Editing};
use wall_proto::{Request, Response, rpc};

use crate::composition::context::Ctx;
use crate::infrastructure::rpc::{HANDLER_FAILED, INVALID_PARAMS};

const OUTPUT_SEPARATOR: char = ',';
const TEXT_ROLE: &str = "surfaceText";
const ACCENT_ROLE: &str = "tertiary";
const SHADE_ROLE: &str = "surface";
const FALLBACK: Palette =
    Palette { text: [236, 235, 231], accent: [157, 184, 255], shade: [18, 19, 22] };
const DAYS: [&str; WEEKDAYS] = ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"];
const DAY_NAMES: [&str; WEEKDAYS] =
    ["MONDAY", "TUESDAY", "WEDNESDAY", "THURSDAY", "FRIDAY", "SATURDAY", "SUNDAY"];
const MONTH_SHORT: [&str; MONTHS] =
    ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
const MONTH_NAMES: [&str; MONTHS] = [
    "JANUARY",
    "FEBRUARY",
    "MARCH",
    "APRIL",
    "MAY",
    "JUNE",
    "JULY",
    "AUGUST",
    "SEPTEMBER",
    "OCTOBER",
    "NOVEMBER",
    "DECEMBER",
];

type Stamp = (u64, Option<SystemTime>);
type Recorded = (Stamp, Option<Stamp>, Option<Stamp>);

struct Desktop {
    state: Weak<WallState>,
    outlines: Mutex<HashMap<PathBuf, (Recorded, Option<PathBuf>)>>,
    wanted: Sender<PathBuf>,
}

impl WidgetSource for Desktop {
    fn scene(&self, shown: &[String]) -> WidgetScene {
        let mut scene = WidgetScene {
            widgets: Vec::new(),
            outlines: Vec::new(),
            palette: FALLBACK,
            font: None,
            labels: labels(),
        };
        let Some(state) = self.state.upgrade() else { return scene };
        let config = state.config();
        scene.widgets = placed(config.root());
        if any_behind(&scene.widgets) {
            scene.outlines = self.outlines(&config, shown);
        }
        scene.palette =
            palette(skwd_wall_core::theme::profiles::live_palette(&state, &config).as_ref());
        scene
    }
}

impl Desktop {
    fn outlines(&self, config: &Config, shown: &[String]) -> Vec<PathBuf> {
        let pack = stamp(&cache::root(config).join(cache::PACK_RECORD));
        let mut memo = lock(&self.outlines);
        memo.retain(|source, _| shown.iter().any(|path| Path::new(path) == source));
        shown
            .iter()
            .filter_map(|path| {
                let source = Path::new(path);
                let index = cache::index_path(config, source).ok();
                let current = (stamp(source)?, index.as_deref().and_then(stamp), pack);
                if let Some((recorded, manifest)) = memo.get(source)
                    && *recorded == current
                {
                    return manifest.clone();
                }
                let manifest = cache::lookup(config, source).ok();
                if manifest.is_none() {
                    let _ = self.wanted.send(source.to_path_buf());
                }
                memo.insert(source.to_path_buf(), (current, manifest.clone()));
                manifest
            })
            .take(MAX_OUTLINES)
            .collect()
    }
}

pub(crate) fn install(ctx: &Ctx) {
    let (wanted, missing) = channel();
    let desktop = Desktop { state: Arc::downgrade(&ctx.state), outlines: Mutex::default(), wanted };
    ctx.state.renderers().set_widget_source(Arc::new(desktop));
    refresh(&ctx.state);
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        for source in missing {
            crate::infrastructure::drawing::outline(&ctx, &source);
        }
    });
}

pub(crate) fn refresh(state: &WallState) {
    if let Err(error) = state.renderers().refresh_widgets() {
        log::warn!("widgets: refresh failed: {error:#}");
    }
}

pub(crate) fn outline_ready(state: &WallState, source: &Path) {
    let source = source.canonicalize().ok();
    let shown = state.renderers().assignments().into_values();
    if shown.into_iter().any(|path| Path::new(&path).canonicalize().ok() == source) {
        refresh(state);
    }
}

pub(crate) fn edit_rpc(req: &Request) -> Response {
    match editing(req) {
        Ok(editing) => Response::ok(req.id, json!(editing)),
        Err(error) => Response::err(req.id, INVALID_PARAMS, error.to_string()),
    }
}

pub(crate) fn edit_request(req: &Request) -> Option<bool> {
    (req.method == rpc::WIDGETS_EDIT).then(|| editing(req).ok()).flatten().map(|edit| edit.active)
}

pub(crate) fn set_editing(state: &WallState, joined: bool) {
    if state.renderers().widget_editor(joined) {
        refresh(state);
    }
}

fn editing(req: &Request) -> serde_json::Result<Editing> {
    serde_json::from_value(req.params.clone())
}

pub(crate) fn scene_rpc(state: &WallState, req: &Request) -> Response {
    let renderers = state.renderers();
    let Some(mut scene) = renderers.committed_widgets(None) else {
        return Response::err(req.id, HANDLER_FAILED, "widgets are not installed");
    };
    let config = state.config();
    let display = config.display();
    let mut outputs = Vec::new();
    let stills = renderers
        .assignments()
        .into_iter()
        .filter(|(_, path)| Path::new(path).is_file() && !paper_control::is_video_path(path));
    for (key, path) in stills {
        for name in key.split(OUTPUT_SEPARATOR) {
            outputs.push(DeskOutput {
                name: name.to_owned(),
                path: path.clone(),
                fill: display.fill_mode_for(name),
                background: display.background_for(name),
            });
        }
    }
    outputs.sort_by(|left, right| left.name.cmp(&right.name));
    let shown: BTreeSet<&str> = outputs.iter().map(|output| output.path.as_str()).collect();
    scene.outlines = shown
        .into_iter()
        .filter_map(|path| cache::lookup(&config, Path::new(path)).ok())
        .take(MAX_OUTLINES)
        .collect();
    match serde_json::to_value(Desk { scene, outputs }) {
        Ok(desk) => Response::ok(req.id, desk),
        Err(error) => Response::err(req.id, HANDLER_FAILED, error.to_string()),
    }
}

fn placed(root: &serde_json::Value) -> Vec<Widget> {
    skwd_config::arr_ref(root, keys::ITEMS)
        .iter()
        .filter_map(|entry| serde_json::from_value::<Widget>(entry.clone()).ok())
        .filter(|widget| widget.validate().is_ok())
        .take(MAX_WIDGETS)
        .collect()
}

pub(crate) fn behind(root: &serde_json::Value) -> bool {
    any_behind(&placed(root))
}

fn any_behind(widgets: &[Widget]) -> bool {
    widgets.iter().any(|widget| widget.depth == Depth::Behind)
}

fn stamp(path: &Path) -> Option<Stamp> {
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.len(), metadata.modified().ok()))
}

fn palette(colors: Option<&Value>) -> Palette {
    let role = |name: &str| {
        let Rgb(red, green, blue) =
            colors?.get(name).and_then(Value::as_str).and_then(parse_hex)?;
        Some([red, green, blue])
    };
    match (role(TEXT_ROLE), role(ACCENT_ROLE), role(SHADE_ROLE)) {
        (Some(text), Some(accent), Some(shade)) => Palette { text, accent, shade },
        _ => FALLBACK,
    }
}

fn labels() -> Labels {
    Labels {
        days: DAYS.map(String::from),
        day_names: DAY_NAMES.map(String::from),
        months: MONTH_SHORT.map(String::from),
        month_names: MONTH_NAMES.map(String::from),
    }
}

#[cfg(test)]
#[path = "widgets_tests.rs"]
mod tests;
