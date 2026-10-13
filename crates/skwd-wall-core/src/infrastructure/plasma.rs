use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;

use super::paper::{
    Assignment, Layer, Source, TransitionPolicy, VideoEngine, tinier_or_default_source,
};
use crate::state::WallState;

pub mod activities;
pub mod channel;
mod presentation;
mod session;

const ASSIGNMENTS_KEY: &str = "Assignments";
const LAST_SCREEN_KEY: &str = "lastScreen";
const WALLPAPER_GROUP: &str = "Wallpaper";
const GENERAL_GROUP: &str = "General";
const PLUGIN_ID: &str = "org.skwd.wall.plasma";
const LOCK_SCREEN_GROUPS: [&str; 4] = ["Greeter", "Wallpaper", PLUGIN_ID, "General"];
const LOCK_SCREEN_CONFIG: &str = "kscreenlockerrc";
const LOCK_SCREEN_RECEIPT: &str = "plasma.json";
const QDBUS_PROGRAMS: &[&str] = &["qdbus6", "qdbus-qt6"];

pub struct LockScreenCurrent<'a> {
    pub kind: &'a str,
    pub path: &'a str,
    pub we_id: &'a str,
    pub poster: &'a str,
}

fn data_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(value) = std::env::var_os("XDG_DATA_HOME") {
        roots.push(PathBuf::from(value));
    } else if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".local/share"));
    }
    roots.extend(
        std::env::var_os("XDG_DATA_DIRS")
            .unwrap_or_else(|| "/usr/local/share:/usr/share".into())
            .to_string_lossy()
            .split(':')
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
    );
    roots
}

fn plugin_installed_in(roots: &[PathBuf]) -> bool {
    roots
        .iter()
        .any(|root| root.join("plasma/wallpapers").join(PLUGIN_ID).join("metadata.json").is_file())
}

fn enabled_for(plasma: bool, roots: &[PathBuf], disabled: bool) -> bool {
    !disabled && plasma && plugin_installed_in(roots)
}

pub fn available() -> bool {
    enabled_for(
        running_on_plasma(),
        &data_roots(),
        std::env::var("SKWD_PLASMA_BACKEND").as_deref() == Ok("0"),
    )
}

fn require_backend_for(plasma: bool, roots: &[PathBuf], disabled: bool) -> anyhow::Result<()> {
    anyhow::ensure!(
        !plasma || disabled || plugin_installed_in(roots),
        "Plasma wallpaper support needs skwd-paper-plasma. Install the plugin and restart Plasma before applying a wallpaper."
    );
    Ok(())
}

pub(crate) fn require_backend() -> anyhow::Result<()> {
    require_backend_for(
        running_on_plasma(),
        &data_roots(),
        std::env::var("SKWD_PLASMA_BACKEND").as_deref() == Ok("0"),
    )
}

fn running_on_plasma() -> bool {
    session::running_on_plasma()
}

fn qdbus_program_in(search_path: Option<&OsStr>) -> Option<PathBuf> {
    crate::paths::resolve_preferred_binary(None, search_path, QDBUS_PROGRAMS)
}

fn qdbus_program() -> Option<PathBuf> {
    qdbus_program_in(std::env::var_os("PATH").as_deref())
}

fn kconfig(program: &str, groups: &[&str], key: &str) -> Command {
    let mut command = Command::new(program);
    command.args(["--file", LOCK_SCREEN_CONFIG]);
    for group in groups {
        command.args(["--group", group]);
    }
    command.args(["--key", key]);
    command
}

fn kconfig_read(groups: &[&str], key: &str) -> anyhow::Result<String> {
    let output = kconfig("kreadconfig6", groups, key)
        .output()
        .with_context(|| format!("read Plasma lock-screen key {key}"))?;
    if !output.status.success() {
        anyhow::bail!("kreadconfig6 failed while reading {key}: {}", output.status);
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn kconfig_write(groups: &[&str], key: &str, value: Option<&str>) -> anyhow::Result<()> {
    let mut command = kconfig("kwriteconfig6", groups, key);
    match value {
        Some(value) => command.arg(value),
        None => command.arg("--delete"),
    };
    let status = command.status().with_context(|| format!("write Plasma lock-screen key {key}"))?;
    if !status.success() {
        anyhow::bail!("kwriteconfig6 failed while writing {key}: {status}");
    }
    Ok(())
}

fn select_lock_screen_plugin(plugin: &str) -> anyhow::Result<()> {
    let previous = kconfig_read(&["Greeter"], "WallpaperPlugin")?;
    if previous != plugin {
        kconfig_write(&LOCK_SCREEN_GROUPS, "PreviousWallpaperPlugin", Some(&previous))?;
    }
    // Select the plugin last so the greeter can never observe half-written plugin configuration.
    kconfig_write(&["Greeter"], "WallpaperPlugin", Some(plugin))
}

fn restored_lock_screen_plugin(previous: &str) -> Option<&str> {
    (!previous.is_empty() && previous != PLUGIN_ID).then_some(previous)
}

fn release_lock_screen() -> anyhow::Result<bool> {
    let record = lock_screen_record();
    if kconfig_read(&["Greeter"], "WallpaperPlugin")? != PLUGIN_ID {
        super::lock_screens::backup::end(&record)?;
        return Ok(false);
    }
    super::lock_screens::backup::begin(
        &super::lock_screens::config_home().join(LOCK_SCREEN_CONFIG),
        &record,
    )?;
    let previous = kconfig_read(&LOCK_SCREEN_GROUPS, "PreviousWallpaperPlugin")?;
    kconfig_write(&["Greeter"], "WallpaperPlugin", restored_lock_screen_plugin(&previous))?;
    notify_lock_screen();
    super::lock_screens::backup::end(&record)?;
    Ok(true)
}

fn lock_screen_record() -> PathBuf {
    crate::paths::data_dir().join(super::lock_screens::DIRECTORY).join(LOCK_SCREEN_RECEIPT)
}

fn notify_lock_screen() {
    let Some(program) = qdbus_program() else {
        log::debug!(
            "could not ask KScreenLocker to reload configuration: qdbus6 or qdbus-qt6 is not in PATH"
        );
        return;
    };
    let status = Command::new(program)
        .args(["org.kde.screensaver", "/ScreenSaver", "org.kde.screensaver.configure"])
        .status();
    match status {
        Ok(status) if status.success() => {}
        Ok(status) => log::debug!("KScreenLocker configuration reload exited with {status}"),
        Err(error) => log::debug!("could not ask KScreenLocker to reload configuration: {error}"),
    }
}

fn lock_screen_stream_size(outputs: &[crate::outputs::OutputInfo]) -> (i32, i32) {
    outputs
        .iter()
        .map(crate::outputs::OutputInfo::logical_size)
        .max_by_key(|(width, height)| i64::from(*width) * i64::from(*height))
        .map_or((1920, 1080), |(width, height)| (width.max(16), height.max(16)))
}

fn use_lock_screen_paper(state: &WallState, current: &LockScreenCurrent<'_>) -> anyhow::Result<()> {
    if !available() {
        anyhow::bail!("the {PLUGIN_ID} Plasma wallpaper plugin is not installed");
    }
    let source = source_for(state, current.kind, current.path, current.we_id);
    if source.is_empty() || !Path::new(&source).exists() {
        anyhow::bail!("lock-screen wallpaper source does not exist: {source}");
    }
    let outputs = crate::outputs::enumerate();
    let (width, height) = lock_screen_stream_size(&outputs);
    let fps = crate::we::scene_fps(state, current.we_id).max(1);
    let groups = LOCK_SCREEN_GROUPS;
    let entry = serde_json::json!({
        "type": current.kind,
        "path": current.path,
        "we_id": current.we_id,
        "mute": true,
        "volume": 0,
    });
    let assignment = paper_assignment(state, "*", &entry)?;
    super::lock_screens::backup::begin(
        &super::lock_screens::config_home().join(LOCK_SCREEN_CONFIG),
        &lock_screen_record(),
    )?;
    kconfig_write(&groups, "Assignment", Some(&serde_json::to_string(&assignment)?))?;
    kconfig_write(&groups, "Paper", Some(&state.config().renderer().paper_bin()))?;
    kconfig_write(&groups, "Paused", Some("false"))?;
    kconfig_write(&groups, "StreamWidth", Some(&width.to_string()))?;
    kconfig_write(&groups, "StreamHeight", Some(&height.to_string()))?;
    kconfig_write(&groups, "StreamFps", Some(&fps.to_string()))?;
    select_lock_screen_plugin(PLUGIN_ID)?;
    notify_lock_screen();
    Ok(())
}

pub fn sync_lock_screen(
    state: &WallState,
    current: Option<&LockScreenCurrent<'_>>,
) -> anyhow::Result<bool> {
    let config = state.config().clone();
    sync_lock_screen_config(state, &config, current)
}

pub fn sync_lock_screen_config(
    state: &WallState,
    config: &crate::config::Config,
    current: Option<&LockScreenCurrent<'_>>,
) -> anyhow::Result<bool> {
    if !running_on_plasma() {
        return Ok(false);
    }
    let mode = config.plasma_lock_screen_mode();
    if matches!(
        mode.as_str(),
        skwd_config::keys::lock_screens::STATIC | skwd_config::keys::lock_screens::FOLLOW
    ) && state.config().plasma_lock_screen_mode() == skwd_config::keys::lock_screens::OFF
    {
        super::lock_screens::backup::end(&lock_screen_record())?;
    }
    match mode.as_str() {
        "static" => {
            let image = config.plasma_lock_screen_image();
            if image.is_empty() {
                anyhow::bail!("Plasma lock-screen image is empty");
            }
            use_lock_screen_paper(
                state,
                &LockScreenCurrent {
                    kind: wall_proto::kind::STATIC,
                    path: &image,
                    we_id: "",
                    poster: "",
                },
            )?;
        }
        "follow" => {
            let Some(current) = current else { return Ok(false) };
            if current.kind == wall_proto::kind::STATIC || config.plasma_lock_screen_live() {
                if let Err(error) = use_lock_screen_paper(state, current) {
                    if current.poster.is_empty() {
                        return Err(error);
                    }
                    log::warn!("live Plasma lock screen unavailable ({error}); using its poster");
                    use_lock_screen_paper(
                        state,
                        &LockScreenCurrent {
                            kind: wall_proto::kind::STATIC,
                            path: current.poster,
                            we_id: "",
                            poster: "",
                        },
                    )?;
                }
            } else {
                if current.poster.is_empty() {
                    anyhow::bail!("the last dynamic wallpaper has no poster frame yet");
                }
                use_lock_screen_paper(
                    state,
                    &LockScreenCurrent {
                        kind: wall_proto::kind::STATIC,
                        path: current.poster,
                        we_id: "",
                        poster: "",
                    },
                )?;
            }
        }
        _ => return release_lock_screen(),
    }
    Ok(true)
}

fn source_for(state: &WallState, kind: &str, path: &str, we_id: &str) -> String {
    if kind == wall_proto::kind::WE {
        if !path.is_empty() {
            return path.to_string();
        }
        return state.config().we_dir().join(we_id).to_string_lossy().into_owned();
    }
    path.to_string()
}

fn assignments(
    state: &WallState,
    outputs: &[crate::outputs::OutputInfo],
    map: &serde_json::Map<String, serde_json::Value>,
    transitions: &std::collections::BTreeMap<String, TransitionPolicy>,
) -> anyhow::Result<serde_json::Value> {
    let mut assignments = serde_json::Map::new();
    for output in outputs {
        let Some(entry) = map.get(&output.name) else { continue };
        let kind = entry.get("type").and_then(serde_json::Value::as_str).unwrap_or("");
        if !matches!(
            kind,
            wall_proto::kind::STATIC | wall_proto::kind::VIDEO | wall_proto::kind::WE
        ) {
            continue;
        }
        let (width, height) = output.logical_size();
        let mut assignment = paper_assignment(state, &output.name, entry)?;
        assignment.transition = transitions.get(&output.name).cloned().map(|mut transition| {
            transition.fps = Some(state.config().transition().output_fps(output.refresh_mhz));
            transition
        });
        assignments.insert(
            output.name.clone(),
            serde_json::json!({
                "assignment": assignment,
                activities::WALLPAPER: entry,
                "paper": state.config().renderer().paper_bin(),
                "width": width.max(16),
                "height": height.max(16),
                "fps": crate::outputs::effective_fps(crate::we::scene_fps(state, entry.get("we_id").and_then(serde_json::Value::as_str).unwrap_or("")), output.refresh_mhz),
                "paused": state.renderers().paused_for(&output.name),
                "manualPaused": state.renderers().manual_paused_for(&output.name),
            }),
        );
    }
    Ok(serde_json::Value::Object(assignments))
}

fn paper_assignment(
    state: &WallState,
    output: &str,
    entry: &serde_json::Value,
) -> anyhow::Result<Assignment> {
    let kind = entry.get("type").and_then(serde_json::Value::as_str).unwrap_or("");
    let path = entry.get("path").and_then(serde_json::Value::as_str).unwrap_or("");
    let we_id = entry.get("we_id").and_then(serde_json::Value::as_str).unwrap_or("");
    let source = match kind {
        wall_proto::kind::STATIC => Source::static_file(path),
        wall_proto::kind::VIDEO if state.config().renderer().video_engine() == "tinier" => {
            tinier_or_default_source(state, path)?
        }
        wall_proto::kind::VIDEO => Source::video(path, Some(VideoEngine::Default)),
        wall_proto::kind::WE => {
            let properties = state
                .with_db(|connection| Ok(crate::db::we_properties(connection, we_id)))
                .unwrap_or_default();
            Source::wallpaper_engine(source_for(state, kind, path, we_id))
                .with_properties(properties)
        }
        _ => anyhow::bail!("unsupported Plasma wallpaper kind {kind}"),
    };
    Ok(Assignment {
        outputs: vec![output.to_string()],
        source,
        fill_mode: state.config().display().fill_mode_for(output).parse().unwrap_or_default(),
        background: state.config().display().background_for(output),
        mute: state.renderers().audio_ducked()
            || entry.get("mute").and_then(serde_json::Value::as_bool).unwrap_or(true),
        volume: entry.get("volume").and_then(serde_json::Value::as_u64).unwrap_or(80).min(100)
            as u32,
        layer: Layer::Background,
        transition: None,
    })
}

fn script(
    payload: &serde_json::Value,
    activity: &str,
    visible: bool,
    connectors: &[String],
) -> anyhow::Result<String> {
    let plugin = PLUGIN_ID;
    let assignments_key = ASSIGNMENTS_KEY;
    let last_screen_key = LAST_SCREEN_KEY;
    let wallpaper_group = WALLPAPER_GROUP;
    let general_group = GENERAL_GROUP;
    let activity = serde_json::to_string(activity)?;
    let connectors = serde_json::to_string(connectors)?;
    Ok(format!(
        r#"var a={payload};var encoded=JSON.stringify(a);var target={activity};
if((currentActivity()===target)!=={visible})throw new Error("Activity changed; retry the apply");
if(activities().indexOf(target)<0)throw new Error("Activity no longer exists");
var screens=Object.keys(a).map(function(output){{var screen=screenForConnector(output);if(screen<0)throw new Error("Unknown output connector");return screen;}});
var before=desktops().map(function(desktop){{return desktop.id;}});
function screenOf(desktop){{desktop.currentConfigGroup=[];return desktop.screen<0?Number(desktop.readConfig("{last_screen_key}",-1)):desktop.screen;}}
function targetDesktops(){{var seen={{}};return desktopsForActivity(target).filter(function(desktop){{if(seen[desktop.id])return false;seen[desktop.id]=true;return true;}});}}
var ds=targetDesktops();
{connectors}.map(function(output){{return screenForConnector(output);}}).filter(function(screen){{return screen>=0;}}).forEach(function(screen){{
    if(ds.some(function(desktop){{return screenOf(desktop)===screen;}}))return;
    var spare=ds.filter(function(desktop){{return screenOf(desktop)<0;}})[0];
    if(!spare){{if(screens.indexOf(screen)>=0)throw new Error("Activity has no desktop for the selected output");return;}}
    spare.screen=screen;
    ds=targetDesktops();
    if(!ds.some(function(desktop){{return screenOf(desktop)===screen;}}))throw new Error("Activity desktop initialization failed");
}});
ds.filter(function(desktop){{return screenOf(desktop)<0&&before.indexOf(desktop.id)<0;}}).forEach(function(desktop){{desktop.remove();}});
ds=targetDesktops();
if(!screens.every(function(screen){{return ds.some(function(desktop){{return screenOf(desktop)===screen;}});}}))throw new Error("Activity has no desktop for the selected output");
ds.forEach(function(d){{
    var selected=screens.indexOf(screenOf(d))>=0;
    var active=d.wallpaperPlugin==="{plugin}";
    if(!active&&!selected)return;
    d.currentConfigGroup=["{wallpaper_group}","{plugin}","{general_group}"];
    var merged=active?JSON.parse(d.readConfig("{assignments_key}","{{}}")):{{}};
    Object.keys(a).forEach(function(output){{merged[output]=a[output];}});
    encoded=JSON.stringify(merged);
    if(active&&d.readConfig("{assignments_key}","")===encoded)return;
    d.writeConfig("{assignments_key}",encoded);
    if(!active)d.wallpaperPlugin="{plugin}";
}});"#
    ))
}

pub fn apply(
    state: &WallState,
    outputs: &[crate::outputs::OutputInfo],
    map: &serde_json::Map<String, serde_json::Value>,
    transitions: &std::collections::BTreeMap<String, TransitionPolicy>,
) -> anyhow::Result<()> {
    let activity = activities::current_target()?;
    let mut payload = assignments(state, outputs, map, transitions)?;
    let presentation = presentation::Pending::new(&mut payload)?;
    activities::evaluate(&script(
        &payload,
        &activity.id,
        true,
        &outputs.iter().map(|output| output.name.clone()).collect::<Vec<_>>(),
    )?)?;
    let transition_ms =
        transitions.values().filter_map(|transition| transition.duration_ms).max().unwrap_or(0);
    presentation.wait(std::time::Duration::from_millis(15_000 + transition_ms.min(60_000)))
}

pub fn save_activity(
    state: &WallState,
    activity: &str,
    output: &str,
    wallpaper: &serde_json::Value,
    override_locks: bool,
) -> anyhow::Result<()> {
    let activity = activities::resolve(Some(activity))?;
    anyhow::ensure!(!activity.current, "Activity became current; retry the apply");
    require_backend()?;
    let kind = wallpaper
        .get(wall_proto::apply_fields::TYPE)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let path = wallpaper
        .get(wall_proto::apply_fields::PATH)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let we_id = wallpaper
        .get(wall_proto::apply_fields::WE_ID)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let mut wallpaper = wallpaper.clone();
    if kind == wall_proto::kind::WE {
        anyhow::ensure!(
            state.config().steam_enabled() && crate::we::valid_we_id(we_id),
            "invalid or disabled Wallpaper Engine item"
        );
        let directory = state.config().we_dir().join(we_id);
        let (project_type, file) = crate::we::read_project_type(&directory);
        anyhow::ensure!(
            directory.is_dir() && crate::we::is_supported_project(&project_type),
            "unsupported Wallpaper Engine project"
        );
        if project_type == wall_proto::kind::VIDEO {
            let video = crate::we::safe_item_join(&directory, &file)
                .filter(|path| path.is_file())
                .context("Wallpaper Engine media does not exist")?;
            wallpaper[wall_proto::apply_fields::TYPE] = serde_json::json!(wall_proto::kind::VIDEO);
            wallpaper[wall_proto::apply_fields::PATH] = serde_json::json!(video);
            wallpaper[wall_proto::apply_fields::WE_ID] = serde_json::json!("");
        }
    } else {
        anyhow::ensure!(
            matches!(kind, wall_proto::kind::STATIC | wall_proto::kind::VIDEO)
                && Path::new(path).is_file(),
            "wallpaper source does not exist"
        );
    }
    let outputs: Vec<_> = crate::outputs::enumerate()
        .into_iter()
        .filter(|candidate| {
            output == wall_proto::apply_fields::ALL_OUTPUTS || candidate.name == output
        })
        .filter(|candidate| {
            override_locks || !state.config().display().output_locked(&candidate.name)
        })
        .collect();
    anyhow::ensure!(!outputs.is_empty(), "Activity apply needs a connected output");
    let map = outputs.iter().map(|output| (output.name.clone(), wallpaper.clone())).collect();
    let payload = assignments(state, &outputs, &map, &std::collections::BTreeMap::new())?;
    activities::evaluate(&script(&payload, &activity.id, false, &crate::outputs::names())?)?;
    Ok(())
}

pub fn apply_current(state: &WallState) -> anyhow::Result<()> {
    apply_current_with_transition(state, "", None)
}

pub fn apply_current_with_transition(
    state: &WallState,
    output: &str,
    transition: Option<TransitionPolicy>,
) -> anyhow::Result<()> {
    let current = crate::audio::read_state(&state.config().cache_dir());
    let Some(map) = current.as_object() else {
        anyhow::bail!("Plasma wallpaper state is empty");
    };
    let outputs = crate::outputs::enumerate();
    let transitions = transition.map_or_else(std::collections::BTreeMap::new, |transition| {
        outputs
            .iter()
            .filter(|candidate| output == "*" || candidate.name == output)
            .map(|candidate| (candidate.name.clone(), transition.clone()))
            .collect()
    });
    apply(state, &outputs, map, &transitions)
}

pub fn retire_native(state: &WallState) {
    state.renderers().kill_base_still();
    state.renderers().kill_output_stills();
    state.renderers().kill_video_papers();
    state.renderers().kill_paper();
    state.renderers().kill_holders();
}

#[cfg(test)]
mod tests;
