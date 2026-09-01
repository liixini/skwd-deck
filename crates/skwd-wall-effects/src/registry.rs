use image::DynamicImage;
use serde_json::Value;

pub struct EffectDef {
    pub id: &'static str,
    pub schema: fn() -> Value,
    pub render: fn(DynamicImage, &Value) -> anyhow::Result<DynamicImage>,
}

fn categorized() -> Vec<(&'static str, Vec<EffectDef>)> {
    vec![
        ("Colour", super::color::effects()),
        ("Tone", super::tone::effects()),
        ("Stylize", super::stylize::effects()),
        ("Distort", super::distort::effects()),
    ]
}

#[must_use]
pub fn all() -> Vec<EffectDef> {
    categorized().into_iter().flat_map(|(_, defs)| defs).collect()
}

#[must_use]
pub fn find(id: &str) -> Option<EffectDef> {
    all().into_iter().find(|def| def.id == id)
}

#[must_use]
pub fn schemas() -> Vec<Value> {
    categorized()
        .into_iter()
        .flat_map(|(cat, defs)| {
            defs.into_iter().map(move |def| {
                let mut schema = (def.schema)();
                schema["category"] = Value::String(cat.to_string());
                schema
            })
        })
        .collect()
}

mod tests;
