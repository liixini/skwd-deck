use anyhow::Context;
use serde_json::{Map, Value};
use wall_proto::{ActivitiesResult, Activity};

const SERVICE: &str = "org.kde.ActivityManager";
const OBJECT: &str = "/ActivityManager/Activities";
const LIST: &str = "org.kde.ActivityManager.Activities.ListActivities";
const CURRENT: &str = "org.kde.ActivityManager.Activities.CurrentActivity";
const NAME: &str = "org.kde.ActivityManager.Activities.ActivityName";
const SHELL_SERVICE: &str = "org.kde.plasmashell";
const SHELL_OBJECT: &str = "/PlasmaShell";
const EVALUATE: &str = "org.kde.PlasmaShell.evaluateScript";
pub(super) const WALLPAPER: &str = "wallpaper";

fn call(args: &[&str]) -> anyhow::Result<String> {
    let program = super::qdbus_program().context("find qdbus6 or qdbus-qt6 in PATH")?;
    let output = std::process::Command::new(program).args(args).output()?;
    anyhow::ensure!(
        output.status.success(),
        "Plasma D-Bus call failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

pub fn list() -> anyhow::Result<ActivitiesResult> {
    if !super::running_on_plasma() {
        return Ok(ActivitiesResult {
            supported: false,
            current: String::new(),
            activities: Vec::new(),
        });
    }
    let current = call(&[SERVICE, OBJECT, CURRENT])?;
    let ids = call(&[SERVICE, OBJECT, LIST])?;
    let activities = ids
        .lines()
        .filter(|id| !id.is_empty())
        .map(|id| {
            Ok(Activity {
                id: id.to_string(),
                name: call(&[SERVICE, OBJECT, NAME, id])?,
                current: id == current,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    anyhow::ensure!(
        !current.is_empty() && activities.iter().any(|activity| activity.current),
        "Plasma has no current Activity"
    );
    Ok(ActivitiesResult { supported: true, current, activities })
}

pub fn resolve(requested: Option<&str>) -> anyhow::Result<Activity> {
    let activities = list()?;
    anyhow::ensure!(activities.supported, "Activities require Plasma");
    let id = requested.unwrap_or(&activities.current);
    activities
        .activities
        .into_iter()
        .find(|activity| activity.id == id)
        .with_context(|| format!("unknown or deleted Plasma Activity: {id}"))
}

pub(super) fn evaluate(script: &str) -> anyhow::Result<String> {
    call(&[SHELL_SERVICE, SHELL_OBJECT, EVALUATE, script])
}

pub fn wallpapers(activity: &str) -> anyhow::Result<Map<String, Value>> {
    resolve(Some(activity))?;
    let id = serde_json::to_string(activity)?;
    let plugin = super::PLUGIN_ID;
    let assignments_key = super::ASSIGNMENTS_KEY;
    let last_screen_key = super::LAST_SCREEN_KEY;
    let wallpaper_group = super::WALLPAPER_GROUP;
    let general_group = super::GENERAL_GROUP;
    let script = format!(
        r#"var target={id};var result={{}};desktopsForActivity(target).forEach(function(d){{if(d.wallpaperPlugin!=="{plugin}")return;d.currentConfigGroup=[];var screen=d.screen>=0?d.screen:Number(d.readConfig("{last_screen_key}",-1));d.currentConfigGroup=["{wallpaper_group}","{plugin}","{general_group}"];var entries=JSON.parse(d.readConfig("{assignments_key}","{{}}"));Object.keys(entries).forEach(function(output){{if(screen>=0&&screenForConnector(output)===screen)result[output]=entries[output];}});}});print(JSON.stringify(result));"#
    );
    let entries: Map<String, Value> = serde_json::from_str(&evaluate(&script)?)?;
    entries
        .into_iter()
        .map(|(output, entry)| {
            let wallpaper = match entry.get(WALLPAPER) {
                Some(wallpaper) => wallpaper.clone(),
                None => legacy_wallpaper(&entry)?,
            };
            Ok((output, wallpaper))
        })
        .collect()
}

pub fn forget_wallpaper(
    source: &std::path::Path,
    we_id: &str,
    actual: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    if !super::available() {
        return Ok(());
    }
    evaluate(&forget_script(source, we_id, actual)?)?;
    Ok(())
}

fn forget_script(
    source: &std::path::Path,
    we_id: &str,
    actual: Option<&std::path::Path>,
) -> anyhow::Result<String> {
    let id = serde_json::to_string(we_id)?;
    let source = source.join(we_id);
    let folder = serde_json::to_string(&source.to_string_lossy())?;
    let actual = actual
        .map_or_else(|| source.canonicalize().unwrap_or(source), std::path::Path::to_path_buf);
    let actual = serde_json::to_string(&actual.to_string_lossy())?;
    let plugin = super::PLUGIN_ID;
    let assignments_key = super::ASSIGNMENTS_KEY;
    let wallpaper_group = super::WALLPAPER_GROUP;
    let general_group = super::GENERAL_GROUP;
    let script = format!(
        r#"var id={id};var folder={folder};var actual={actual};
activities().forEach(function(activity){{desktopsForActivity(activity).forEach(function(d){{
if(d.wallpaperPlugin!=="{plugin}")return;
d.currentConfigGroup=["{wallpaper_group}","{plugin}","{general_group}"];
var entries=JSON.parse(d.readConfig("{assignments_key}","{{}}"));var changed=false;
Object.keys(entries).forEach(function(output){{var item=entries[output].wallpaper||entries[output];var path=item.path||"";
if(item.we_id===id||path===folder||path.indexOf(folder+"/")===0||path===actual||path.indexOf(actual+"/")===0){{delete entries[output];changed=true;}}
}});
if(changed)d.writeConfig("{assignments_key}",JSON.stringify(entries));
}});}});"#
    );
    Ok(script)
}

fn legacy_wallpaper(entry: &Value) -> anyhow::Result<Value> {
    let assignment: super::Assignment =
        serde_json::from_value(entry[wall_proto::apply_fields::ASSIGNMENT].clone())?;
    let kind = match assignment.source.kind {
        paper_control::SourceKind::Static => wall_proto::kind::STATIC,
        paper_control::SourceKind::Video => wall_proto::kind::VIDEO,
        paper_control::SourceKind::WallpaperEngine => wall_proto::kind::WE,
    };
    let we_id = if kind == wall_proto::kind::WE {
        std::path::Path::new(&assignment.source.path)
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default()
    } else {
        ""
    };
    Ok(crate::audio::entry(
        kind,
        &assignment.source.path,
        we_id,
        assignment.mute,
        assignment.volume,
    ))
}

thread_local! {
    static TARGET: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

pub struct Target {
    id: String,
    previous: Option<String>,
    local: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl Target {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn enter(activity: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(resolve(Some(activity))?.current, "Activity changed; retry the apply");
        let previous = TARGET.with(|target| target.replace(Some(activity.to_string())));
        Ok(Self { id: activity.to_string(), previous, local: std::marker::PhantomData })
    }
}

impl Drop for Target {
    fn drop(&mut self) {
        TARGET.with(|target| *target.borrow_mut() = self.previous.take());
    }
}

pub(super) fn current_target() -> anyhow::Result<Activity> {
    let target = TARGET.with(|target| target.borrow().clone());
    let activity = resolve(target.as_deref())?;
    anyhow::ensure!(activity.current, "Activity changed; retry the apply");
    Ok(activity)
}

pub fn sync_current(state: &crate::WallState) -> anyhow::Result<Option<Target>> {
    if !super::available() {
        return Ok(None);
    }
    let activity = current_target()?;
    let target = Target::enter(&activity.id)?;
    let saved = wallpapers(&activity.id)?;
    crate::audio::write_state(&state.config().cache_dir(), &Value::Object(saved));
    Ok(Some(target))
}

#[cfg(test)]
mod tests;
