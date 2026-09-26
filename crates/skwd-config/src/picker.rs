use serde_json::{Map, Value};

pub fn canonicalize_depth_layout(root: &mut Value) {
    let duration = crate::num_at(root, crate::keys::motion::STANDARD_MS, 250.0).clamp(35.0, 2000.0);
    let Some(selector) =
        root.pointer_mut("/components/wallpaperSelector").and_then(Value::as_object_mut)
    else {
        return;
    };
    convert_depth_units(selector, duration);
    if let Some(presets) =
        selector.get_mut("presets").and_then(|p| p.get_mut("depth")).and_then(Value::as_array_mut)
    {
        for preset in presets {
            if let Some(params) = preset.get_mut("params").and_then(Value::as_object_mut) {
                convert_depth_units(params, duration);
            }
        }
    }
}

fn convert_depth_units(params: &mut Map<String, Value>, duration: f64) {
    if !["depthWidth", "depthSpacing", "depthFalloff", "depthSpeed"]
        .iter()
        .any(|key| params.contains_key(*key))
    {
        return;
    }
    let number = |key: &str, default: f64, min: f64, max: f64| {
        params.get(key).and_then(Value::as_f64).unwrap_or(default).clamp(min, max)
    };
    let height = number("depthHeight", 520.0, 100.0, 1600.0);
    let width = height * number("depthWidth", 54.0, 20.0, 150.0) / 100.0;
    let spacing = width * number("depthSpacing", 86.0, 30.0, 160.0) / 100.0;
    let falloff = number("depthFalloff", 28.0, 0.0, 100.0) / 100.0;
    let navigation =
        (duration * 100.0 / number("depthSpeed", 100.0, 25.0, 300.0)).clamp(35.0, 8000.0);
    for (old, new, value) in [
        ("depthWidth", "depthWidthPx", width),
        ("depthSpacing", "depthSpacingPx", spacing),
        ("depthFalloff", "depthFalloffFactor", falloff),
        ("depthSpeed", "depthNavigationMs", navigation),
    ] {
        params.remove(old);
        params.entry(new).or_insert_with(|| Value::from(value));
    }
}

#[cfg(test)]
#[path = "picker_tests.rs"]
mod tests;
