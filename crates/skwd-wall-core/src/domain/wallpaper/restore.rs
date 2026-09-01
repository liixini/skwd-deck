#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputPolicy {
    Pin(Wallpaper),
    FollowDimension,
}

impl OutputPolicy {
    pub fn as_key(&self) -> &'static str {
        match self {
            Self::Pin(_) => "pin",
            Self::FollowDimension => "follow-dimension",
        }
    }

    pub fn parse(mode: &str, pinned: Option<Wallpaper>) -> Option<Self> {
        match mode {
            "pin" => pinned.map(Self::Pin),
            "follow-dimension" => Some(Self::FollowDimension),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Wallpaper {
    pub kind: String,
    pub path: String,
    pub we_id: String,
}

impl Wallpaper {
    pub fn is_empty(&self) -> bool {
        if self.kind == wall_proto::kind::WE { self.we_id.is_empty() } else { self.path.is_empty() }
    }

    pub fn assigned(&self) -> &str {
        if self.kind == wall_proto::kind::WE { &self.we_id } else { &self.path }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LastApplied {
    pub any: Option<Wallpaper>,
    pub landscape: Option<Wallpaper>,
    pub portrait: Option<Wallpaper>,
}

impl LastApplied {
    pub fn for_orientation(&self, portrait: bool) -> Option<&Wallpaper> {
        let oriented = if portrait { self.portrait.as_ref() } else { self.landscape.as_ref() };
        oriented.or(self.any.as_ref())
    }

    pub fn record(&mut self, wallpaper: &Wallpaper, portrait: bool) {
        if wallpaper.is_empty() {
            return;
        }
        self.any = Some(wallpaper.clone());
        if portrait {
            self.portrait = Some(wallpaper.clone());
        } else {
            self.landscape = Some(wallpaper.clone());
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputTargetState {
    pub output: String,
    pub portrait: bool,
    pub policy: OutputPolicy,
    pub live: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
    pub output: String,
    pub wallpaper: Wallpaper,
    pub already_applied: bool,
}

pub fn resolve(outputs: &[OutputTargetState], last: &LastApplied) -> Vec<Resolution> {
    outputs
        .iter()
        .filter_map(|target| {
            let wallpaper = match &target.policy {
                OutputPolicy::Pin(pinned) => Some(pinned.clone()),
                OutputPolicy::FollowDimension => last.for_orientation(target.portrait).cloned(),
            }
            .filter(|wallpaper| !wallpaper.is_empty())?;
            let already_applied = target
                .live
                .as_deref()
                .is_some_and(|live| !live.is_empty() && live == wallpaper.assigned());
            Some(Resolution { output: target.output.clone(), wallpaper, already_applied })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyGroup {
    pub target: String,
    pub wallpaper: Wallpaper,
}

pub fn group(resolutions: &[Resolution], live: &[String]) -> Vec<ApplyGroup> {
    let mut groups: Vec<(Wallpaper, Vec<String>)> = Vec::new();
    for resolution in resolutions {
        match groups.iter_mut().find(|(wallpaper, _)| *wallpaper == resolution.wallpaper) {
            Some((_, outputs)) => outputs.push(resolution.output.clone()),
            None => groups.push((resolution.wallpaper.clone(), vec![resolution.output.clone()])),
        }
    }
    let covers_every_output = |outputs: &[String]| !live.is_empty() && outputs.len() == live.len();
    groups
        .into_iter()
        .map(|(wallpaper, mut outputs)| {
            outputs.sort();
            let target =
                if covers_every_output(&outputs) { String::from("*") } else { outputs.join(",") };
            ApplyGroup { target, wallpaper }
        })
        .collect()
}

pub fn pending(resolutions: &[Resolution], live: &[String]) -> Vec<ApplyGroup> {
    if resolutions.iter().all(|resolution| resolution.already_applied) {
        return Vec::new();
    }
    group(resolutions, live)
}

#[cfg(test)]
#[path = "restore_tests.rs"]
mod tests;
