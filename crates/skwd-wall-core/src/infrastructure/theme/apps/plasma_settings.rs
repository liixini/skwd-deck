use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use super::{files, manager::Environment, plasma};

const KEYS: [&str; 2] = ["accentColorFromWallpaper", "AccentColor"];
const MANAGED: [&str; 2] = ["false", "0,0,0,0"];

#[derive(Deserialize, Serialize)]
pub(super) struct Accent {
    previous: [Option<String>; 2],
}

pub(super) fn entry(text: &str, group: &str, key: &str) -> Result<Option<String>> {
    let mut selected = false;
    let mut value = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            ensure!(line != format!("[{group}][$i]"), "The Plasma {group} settings are locked");
            selected = line == format!("[{group}]");
        } else if selected && let Some((name, next)) = line.split_once('=') {
            let name = name.trim();
            ensure!(
                !name.starts_with(&format!("{key}[")),
                "The Plasma {key} setting has unsupported configuration flags"
            );
            if name == key {
                ensure!(value.is_none(), "The Plasma {key} setting occurs more than once");
                value = Some(next.trim().into());
            }
        }
    }
    Ok(value)
}

impl Accent {
    pub(super) fn capture(env: &Environment) -> Result<Self> {
        let text = files::read(&env.config.join("kdeglobals"))?.unwrap_or_default();
        for directory in &env.config_dirs {
            let system = files::read(&directory.join("kdeglobals"))?.unwrap_or_default();
            for key in KEYS {
                entry(&system, "General", key)?;
            }
        }
        Ok(Self {
            previous: [entry(&text, "General", KEYS[0])?, entry(&text, "General", KEYS[1])?],
        })
    }

    pub(super) fn owned(&self, env: &Environment, pending: bool) -> Result<bool> {
        let current = Self::capture(env)?;
        Ok(current.previous.iter().enumerate().all(|(index, value)| {
            value.as_deref() == Some(MANAGED[index]) || pending && *value == self.previous[index]
        }))
    }

    pub(super) fn reconnect(&mut self, env: &Environment) -> Result<()> {
        let current = Self::capture(env)?;
        for (index, value) in current.previous.into_iter().enumerate() {
            if value.as_deref() != Some(MANAGED[index]) {
                self.previous[index] = value;
            }
        }
        Ok(())
    }

    pub(super) fn wallpaper_owned(&self, env: &Environment) -> Result<bool> {
        let current = Self::capture(env)?;
        Ok(current.previous[0].as_deref() == Some(MANAGED[0])
            || current.previous[0] == self.previous[0])
    }

    pub(super) fn restore_wallpaper(&self, env: &Environment) -> Result<()> {
        ensure!(
            self.wallpaper_owned(env)?,
            "The Plasma wallpaper accent setting changed outside Skwd"
        );
        Self::write(env, 0, self.previous[0].as_deref())
    }

    pub(super) fn apply(&self, env: &Environment, restore_color: bool) -> Result<()> {
        ensure!(
            self.owned(env, true)?,
            "The Plasma accent changed outside Skwd; reconnect to use Skwd colours"
        );
        Self::write(env, 0, Some(MANAGED[0]))?;
        Self::write(
            env,
            1,
            if restore_color { self.previous[1].as_deref() } else { Some(MANAGED[1]) },
        )
    }

    fn write(env: &Environment, index: usize, value: Option<&str>) -> Result<()> {
        files::writable(&env.config.join("kdeglobals"))?;
        let key = KEYS[index];
        let mut args = vec!["--file", "kdeglobals", "--group", "General", "--key", key, "--notify"];
        if let Some(value) = value {
            args.push(value);
        } else {
            args.push("--delete");
        }
        plasma::run_tool(env, "kwriteconfig6", &args)?;
        let current = Self::capture(env)?;
        ensure!(
            current.previous[index].as_deref() == value,
            "Plasma did not save the {key} setting"
        );
        Ok(())
    }
}

pub(super) fn panel_detail(env: &Environment) -> Result<String> {
    let mut style = None;
    for root in std::iter::once(&env.config).chain(env.config_dirs.iter()) {
        let text = files::read(&root.join("plasmarc"))?.unwrap_or_default();
        style = entry(&text, "Theme", "name")?;
        if style.is_some() {
            break;
        }
    }
    let style = style.unwrap_or_else(|| "default".into());
    ensure!(
        !style.is_empty() && !style.contains(['/', '\\']) && style != "." && style != "..",
        "The Plasma style name is invalid"
    );
    let directory = std::iter::once(&env.data)
        .chain(env.data_dirs.iter())
        .map(|root| root.join("plasma/desktoptheme").join(&style))
        .find(|path| path.is_dir());
    Ok(if directory.is_some_and(|path| path.join("colors").is_file()) {
        format!(
            "The Plasma style {style} supplies its own panel colours. Choose a Plasma style that follows the system colour scheme to colour panels with Skwd."
        )
    } else {
        "Accent colours follow Skwd. Panels follow Skwd when the Plasma style supports system colours.".into()
    })
}
