use serde_json::Value;

use crate::keys::selector as key;
use crate::schema::Setting;
use crate::schema::setting::selector as setting;

pub const TOP_LEFT: usize = 0;
pub const TOP_RIGHT: usize = 1;
pub const BOTTOM_RIGHT: usize = 2;
pub const BOTTOM_LEFT: usize = 3;
pub const CORNERS: usize = 4;
pub const STRAIGHT: &str = "straight";
pub const ROUND: &str = "round";
pub const CHAMFERED: &str = "chamfered";

const SLICE_RADIUS: Setting<f64> = Setting::number(key::CORNER_RADIUS, 16.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CornerStyle {
    Straight,
    Round,
    Chamfered,
}

impl CornerStyle {
    pub const ALL: [Self; 3] = [Self::Straight, Self::Round, Self::Chamfered];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Straight => STRAIGHT,
            Self::Round => ROUND,
            Self::Chamfered => CHAMFERED,
        }
    }

    pub fn from_key(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|style| style.key() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Corners {
    pub style: CornerStyle,
    pub same: bool,
    pub all: f64,
    pub values: [f64; CORNERS],
}

impl Corners {
    pub fn sizes(&self) -> [f64; CORNERS] {
        match (self.style, self.same) {
            (CornerStyle::Straight, _) => [0.0; CORNERS],
            (_, true) => [self.all; CORNERS],
            (_, false) => self.values,
        }
    }

    pub fn shared(&self) -> f64 {
        if self.style == CornerStyle::Straight { 0.0 } else { self.all }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CornerKeys {
    pub style: &'static str,
    pub same: &'static str,
    pub all: &'static str,
    pub values: [&'static str; CORNERS],
    pub radius: Setting<f64>,
    pub round: Option<Setting<bool>>,
}

pub const SLICES: CornerKeys = CornerKeys {
    style: key::CORNER_STYLE,
    same: key::CORNERS_SAME,
    all: key::CORNER_ALL,
    values: [key::CORNER_TL, key::CORNER_TR, key::CORNER_BR, key::CORNER_BL],
    radius: SLICE_RADIUS,
    round: Some(setting::ROUND_CORNERS),
};

pub const GRID: CornerKeys = CornerKeys {
    style: key::GRID_CORNER_STYLE,
    same: key::GRID_CORNERS_SAME,
    all: key::GRID_CORNER_ALL,
    values: [key::GRID_CORNER_TL, key::GRID_CORNER_TR, key::GRID_CORNER_BR, key::GRID_CORNER_BL],
    radius: setting::GRID_CORNER_RADIUS,
    round: Some(setting::GRID_ROUND_CORNERS),
};

pub const HAND: CornerKeys = CornerKeys {
    style: key::HAND_CORNER_STYLE,
    same: key::HAND_CORNERS_SAME,
    all: key::HAND_CORNER_ALL,
    values: [key::HAND_CORNER_TL, key::HAND_CORNER_TR, key::HAND_CORNER_BR, key::HAND_CORNER_BL],
    radius: setting::HAND_CORNER_RADIUS,
    round: None,
};

pub const DEPTH: CornerKeys = CornerKeys {
    style: key::DEPTH_CORNER_STYLE,
    same: key::DEPTH_CORNERS_SAME,
    all: key::DEPTH_CORNER_ALL,
    values: [
        key::DEPTH_CORNER_TL,
        key::DEPTH_CORNER_TR,
        key::DEPTH_CORNER_BR,
        key::DEPTH_CORNER_BL,
    ],
    radius: setting::DEPTH_CORNERS,
    round: None,
};

pub const COLLECTION: CornerKeys = CornerKeys {
    style: key::COLLECTION_CORNER_STYLE,
    same: key::COLLECTION_CORNERS_SAME,
    all: key::COLLECTION_CORNER_ALL,
    values: [
        key::COLLECTION_CORNER_TL,
        key::COLLECTION_CORNER_TR,
        key::COLLECTION_CORNER_BR,
        key::COLLECTION_CORNER_BL,
    ],
    radius: setting::COLLECTION_CORNERS,
    round: None,
};

pub const ALL: [CornerKeys; 5] = [SLICES, GRID, HAND, DEPTH, COLLECTION];

impl CornerKeys {
    pub fn read(&self, root: &Value) -> Corners {
        let legacy = self.radius.read(root);
        let values = self.values.map(|path| {
            crate::get(root, path)
                .and_then(|_| crate::schema::read_number(root, path))
                .unwrap_or(legacy)
        });
        let style = crate::get(root, self.style)
            .and_then(Value::as_str)
            .and_then(CornerStyle::from_key)
            .unwrap_or_else(|| self.legacy_style(root, legacy));
        let stored =
            crate::get(root, self.all).and_then(|_| crate::schema::read_number(root, self.all));
        let equal = values.iter().all(|value| *value == values[TOP_LEFT]);
        let same = match stored {
            Some(_) => crate::get(root, self.same).and_then(Value::as_bool).unwrap_or(equal),
            None => equal,
        };
        let all = stored.unwrap_or(values[TOP_LEFT]);
        Corners { style, same, all, values }
    }

    fn legacy_style(&self, root: &Value, legacy: f64) -> CornerStyle {
        let round = self.round.map_or(legacy > 0.0, |round| round.read(root));
        if round { CornerStyle::Round } else { CornerStyle::Straight }
    }

    pub fn owns(&self, path: &str) -> bool {
        path == self.style || path == self.same || path == self.all || self.values.contains(&path)
    }

    pub fn corner(&self, path: &str) -> Option<usize> {
        self.values.iter().position(|value| *value == path)
    }
}

pub fn owner(path: &str) -> Option<&'static CornerKeys> {
    ALL.iter().find(|keys| keys.owns(path))
}

pub fn number(root: &Value, path: &str) -> Option<f64> {
    let keys = owner(path)?;
    let read = keys.read(root);
    if path == keys.all {
        return Some(read.all);
    }
    keys.corner(path).map(|at| read.values[at])
}

pub fn style(root: &Value, path: &str) -> Option<&'static str> {
    ALL.iter().find(|keys| keys.style == path).map(|keys| keys.read(root).style.key())
}

pub fn same(root: &Value, path: &str) -> Option<bool> {
    ALL.iter().find(|keys| keys.same == path).map(|keys| keys.read(root).same)
}

#[cfg(test)]
#[path = "corners_tests.rs"]
mod tests;
