use std::collections::HashMap;

pub fn template(name: &str) -> Option<&'static str> {
    match name {
        "kitty.conf" => Some(include_str!("../../../../../data/app-themes/legacy/kitty.conf")),
        "yazi-theme.toml" => {
            Some(include_str!("../../../../../data/app-themes/legacy/yazi-theme.toml"))
        }
        _ => None,
    }
}

pub fn generated(name: &str, text: &str) -> bool {
    let Some(source) = template(name) else { return false };
    matches(source, text)
        || name == "yazi-theme.toml"
            && matches(include_str!("../../../../../data/matugen/templates/yazi-theme.toml"), text)
}

fn matches(mut source: &str, text: &str) -> bool {
    let mut output = text;
    let mut colors = HashMap::new();
    while let Some((literal, remainder)) = source.split_once("{{") {
        let Some((token, remainder)) = remainder.split_once("}}") else { return false };
        let Some(rest) = output.strip_prefix(literal) else { return false };
        let Some(color) = rest.get(..7).filter(|value| {
            value.starts_with('#') && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        }) else {
            return false;
        };
        if colors.insert(token, color).is_some_and(|previous| previous != color) {
            return false;
        }
        source = remainder;
        output = &rest[7..];
    }
    output == source
}

#[cfg(test)]
#[path = "legacy_tests.rs"]
mod tests;
