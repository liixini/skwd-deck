use anyhow::{Context, Result, ensure};
use jsonc_parser::cst::{CstInputValue, CstObject, CstRootNode};
use serde_json::Value;

pub(super) enum Document {
    Json(CstRootNode),
    Toml(toml_edit::DocumentMut),
}

fn unique(object: &CstObject) -> Result<()> {
    let mut names = std::collections::HashSet::new();
    for property in object.properties() {
        let name = property.name().context("Missing setting name")?.decoded_value()?;
        ensure!(names.insert(name.clone()), "Duplicate setting {name} needs review");
        if let Some(child) = property.object_value() {
            unique(&child)?;
        }
    }
    Ok(())
}

fn input(value: Value) -> CstInputValue {
    match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(value) => CstInputValue::Bool(value),
        Value::Number(value) => CstInputValue::Number(value.to_string()),
        Value::String(value) => CstInputValue::String(value),
        Value::Array(values) => CstInputValue::Array(values.into_iter().map(input).collect()),
        Value::Object(values) => CstInputValue::Object(
            values.into_iter().map(|(key, value)| (key, input(value))).collect(),
        ),
    }
}

impl Document {
    pub(super) fn parse(text: &str, json: bool) -> Result<Self> {
        if json {
            let root = CstRootNode::parse(text, &jsonc_parser::ParseOptions::default())?;
            let object = root.object_value_or_create().context("Settings must be an object")?;
            unique(&object)?;
            Ok(Self::Json(root))
        } else {
            Ok(Self::Toml(text.parse()?))
        }
    }

    pub(super) fn get(&self, path: &[String]) -> Result<Option<String>> {
        match self {
            Self::Json(root) => {
                let mut object = root.object_value().context("Missing settings object")?;
                for key in &path[..path.len() - 1] {
                    if object.get(key).is_none() {
                        return Ok(None);
                    }
                    object = object.object_value(key).context("Setting group is not an object")?;
                }
                Ok(object.get(&path[path.len() - 1]).and_then(|p| p.value()).map(|v| v.to_string()))
            }
            Self::Toml(root) => {
                let mut item = root.as_item();
                for key in path {
                    let Some(next) = item.get(key) else { return Ok(None) };
                    item = next;
                }
                ensure!(item.is_value(), "Theme setting must be a value");
                Ok(Some(item.to_string()))
            }
        }
    }

    pub(super) fn set(&mut self, path: &[String], value: Option<&str>) -> Result<()> {
        let (key, parents) = path.split_last().context("Missing setting path")?;
        match self {
            Self::Json(root) => {
                let mut object = root.object_value().context("Missing settings object")?;
                for parent in parents {
                    object = object
                        .object_value_or_create(parent)
                        .context("Setting group is not an object")?;
                }
                if let Some(value) = value {
                    let parsed: Value = jsonc_parser::parse_to_serde_value(
                        value,
                        &jsonc_parser::ParseOptions::default(),
                    )?;
                    if let Some(property) = object.get(key) {
                        property.set_value(input(parsed));
                    } else {
                        object.append(key, input(parsed));
                    }
                } else if let Some(property) = object.get(key) {
                    property.remove();
                }
            }
            Self::Toml(root) => {
                let mut item = root.as_item_mut();
                for parent in parents {
                    if item.get(parent).is_none() {
                        item[parent] = toml_edit::Item::Table(toml_edit::Table::new());
                    }
                    item = item.get_mut(parent).context("Missing settings table")?;
                    ensure!(item.as_table_like().is_some(), "Setting group is not a table");
                }
                let table = item.as_table_like_mut().context("Settings must be a table")?;
                if let Some(value) = value {
                    table.insert(key, toml_edit::Item::Value(value.trim().parse()?));
                } else {
                    table.remove(key);
                }
            }
        }
        Ok(())
    }

    pub(super) fn text(&self) -> String {
        match self {
            Self::Json(root) => root.to_string(),
            Self::Toml(root) => root.to_string(),
        }
    }
}
