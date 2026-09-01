use crate::config::Config;
use crate::state::WallState;

pub struct SinkSpec {
    pub name: &'static str,
    pub preview: fn(&WallState, &str, u64) -> anyhow::Result<()>,
    pub preview_end: fn(&WallState),
    pub forget: fn(&WallState),
    pub arm: Option<fn(&WallState)>,
    pub restore_stale: Option<fn(&Config)>,
}

pub const SINKS: [SinkSpec; 3] = [
    SinkSpec {
        name: "noctalia",
        preview: crate::noctalia::preview,
        preview_end: crate::noctalia::preview_end,
        forget: crate::noctalia::preview_end,
        arm: None,
        restore_stale: Some(crate::noctalia::restore_stale_preview),
    },
    SinkSpec {
        name: "dms",
        preview: crate::dms::preview,
        preview_end: crate::dms::preview_end,
        forget: crate::dms::preview_end,
        arm: None,
        restore_stale: Some(crate::dms::restore_stale_preview),
    },
    SinkSpec {
        name: "bridge",
        preview: crate::bridge_preview::preview,
        preview_end: crate::bridge_preview::preview_end,
        forget: crate::bridge_preview::forget,
        arm: Some(crate::bridge_preview::arm),
        restore_stale: None,
    },
];

pub fn active(backend: &str) -> &'static SinkSpec {
    SINKS.iter().find(|sink| sink.name == backend).unwrap_or(&SINKS[2])
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
