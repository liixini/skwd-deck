pub use skwd_palette::material::*;

pub fn document_from_modes(cli: &serde_json::Value, dark: bool) -> Option<serde_json::Value> {
    let dark_tokens = cli.get("dark")?.as_object()?;
    let light_tokens = cli.get("light")?.as_object()?;
    if !crate::material::ROLE_KEYS.iter().all(|key| dark_tokens.contains_key(*key))
        || dark_tokens.len() != light_tokens.len()
    {
        return None;
    }
    let mut colors = serde_json::Map::new();
    for (name, value) in dark_tokens {
        let dark_hex = crate::material::parse_seed(value.as_str()?)?;
        let light_hex = crate::material::parse_seed(light_tokens.get(name)?.as_str()?)?;
        colors.insert(
            name.clone(),
            serde_json::json!({"dark": {"color": dark_hex}, "light": {"color": light_hex}}),
        );
    }
    let mut document = serde_json::json!({"colors": colors});
    crate::material::select_mode(&mut document, dark);
    Some(document)
}
