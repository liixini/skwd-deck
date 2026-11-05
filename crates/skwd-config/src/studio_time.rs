use crate::keys::theme;

pub const MINUTES_PER_HOUR: u32 = 60;
const MINUTES_PER_DAY: u32 = 24 * MINUTES_PER_HOUR;
pub const DAY_HOUR: i32 = 7;
pub const NIGHT_HOUR: i32 = 19;
const DAY_START: u32 = DAY_HOUR as u32 * MINUTES_PER_HOUR;
const NIGHT_START: u32 = NIGHT_HOUR as u32 * MINUTES_PER_HOUR;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudioMode {
    Dark,
    Light,
    ByTime,
    FollowTheme,
}

impl StudioMode {
    pub fn from_key(key: &str) -> Self {
        match key {
            theme::MODE_DARK => Self::Dark,
            theme::MODE_LIGHT => Self::Light,
            theme::STUDIO_MODE_BY_TIME => Self::ByTime,
            _ => Self::FollowTheme,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Dark => theme::MODE_DARK,
            Self::Light => theme::MODE_LIGHT,
            Self::ByTime => theme::STUDIO_MODE_BY_TIME,
            Self::FollowTheme => "",
        }
    }
}

pub fn dark_at(minute: u32) -> bool {
    let minute = minute % MINUTES_PER_DAY;
    !(DAY_START..NIGHT_START).contains(&minute)
}

#[cfg(test)]
mod tests;
