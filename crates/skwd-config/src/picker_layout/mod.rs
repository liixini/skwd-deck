const MISSING_LAYOUT_OUTPUT: &str = "missing layout output";
const UNSUPPORTED_LAYOUT_VERSION: &str = "unsupported layout version";
const INVALID_LAYOUT_RELATIONSHIP: &str = "invalid layout relationship";
const UNSUPPORTED_LAYOUT_SETTING: &str = "unsupported layout setting";
const INVALID_LAYOUT_VIEWPORT: &str = "invalid layout viewport";
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

mod keys;
#[cfg(test)]
mod tests;

pub use keys::{optional, optional_paths, owns};

pub const ROOT: &str = "pickerLayouts";
pub const VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Component {
    Picker,
    Bar,
    Search,
    Arrange,
    Swatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reference {
    Screen,
    Picker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Alignment {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Alignment {
    pub fn factors(self) -> [f32; 2] {
        match self {
            Self::TopLeft => [0.0, 0.0],
            Self::Top => [0.5, 0.0],
            Self::TopRight => [1.0, 0.0],
            Self::Left => [0.0, 0.5],
            Self::Center => [0.5, 0.5],
            Self::Right => [1.0, 0.5],
            Self::BottomLeft => [0.0, 1.0],
            Self::Bottom => [0.5, 1.0],
            Self::BottomRight => [1.0, 1.0],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Offset {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub fn valid(self) -> bool {
        self.width.is_finite() && self.height.is_finite() && self.width > 0.0 && self.height > 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub reference: Reference,
    pub alignment: Alignment,
    pub offset: Offset,
}

impl Position {
    pub fn valid_for(self, component: Component) -> bool {
        !(component == Component::Picker && self.reference == Reference::Picker)
            && self.offset.x.is_finite()
            && self.offset.y.is_finite()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutProfile {
    pub viewport: Size,
    pub values: BTreeMap<String, Option<Value>>,
    pub positions: BTreeMap<Component, Position>,
}

impl LayoutProfile {
    pub fn capture(document: &Value, viewport: Size) -> Self {
        let values = keys::paths()
            .map(|path| (path.to_owned(), crate::get(document, path).cloned()))
            .filter(|(path, value)| value.is_some() || !keys::optional(path))
            .collect();
        Self { viewport, values, positions: BTreeMap::new() }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.viewport.valid() {
            return Err(INVALID_LAYOUT_VIEWPORT.to_owned());
        }
        if self.values.keys().any(|path| !owns(path)) {
            return Err(UNSUPPORTED_LAYOUT_SETTING.to_owned());
        }
        for (component, position) in &self.positions {
            if !position.valid_for(*component) {
                return Err(INVALID_LAYOUT_RELATIONSHIP.to_owned());
            }
        }
        Ok(())
    }

    pub fn set(&mut self, path: &str, value: Option<Value>) {
        if value.is_none() && keys::optional(path) {
            self.values.remove(path);
        } else {
            self.values.insert(path.to_owned(), value);
        }
    }

    pub fn apply(&self, document: &mut Value) {
        for (path, value) in &self.values {
            write(document, path, value.as_ref());
        }
        for path in keys::reset_paths().filter(|path| !self.values.contains_key(*path)) {
            write(document, path, None);
        }
    }
}

fn write(document: &mut Value, path: &str, value: Option<&Value>) {
    let mut parts = path.split('.').peekable();
    let mut node = document;
    while let Some(part) = parts.next() {
        if !node.is_object() {
            *node = Value::Object(serde_json::Map::new());
        }
        let object = node.as_object_mut().expect("layout object");
        if parts.peek().is_none() {
            if let Some(value) = value {
                object.insert(part.to_owned(), value.clone());
            } else {
                object.remove(part);
            }
            break;
        }
        node = object.entry(part).or_insert_with(|| Value::Object(serde_json::Map::new()));
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layouts {
    pub version: u32,
    pub outputs: BTreeMap<String, LayoutProfile>,
}

impl Layouts {
    pub fn read(document: &Value) -> Result<Self, String> {
        let Some(value) = document.get(ROOT) else {
            return Ok(Self { version: VERSION, outputs: BTreeMap::new() });
        };
        let mut layouts: Self =
            serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
        if layouts.version != VERSION {
            return Err(UNSUPPORTED_LAYOUT_VERSION.to_owned());
        }
        for (output, profile) in &mut layouts.outputs {
            keys::retire(&mut profile.values);
            if output.trim().is_empty() {
                return Err(MISSING_LAYOUT_OUTPUT.to_owned());
            }
            profile.validate()?;
        }
        Ok(layouts)
    }
}
