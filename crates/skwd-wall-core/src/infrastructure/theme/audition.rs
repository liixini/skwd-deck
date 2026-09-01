use serde_json::{Value, json};

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct AuditionProfile {
    pub key: String,
    pub value: String,
    pub label: String,
    pub palette: Value,
}

const MATERIAL: [(&str, &str); 9] = [
    ("tonal-spot", "Tonal spot"),
    ("vibrant", "Vibrant"),
    ("expressive", "Expressive"),
    ("neutral", "Neutral"),
    ("monochrome", "Mono"),
    ("fidelity", "Fidelity"),
    ("content", "Content"),
    ("rainbow", "Rainbow"),
    ("fruit-salad", "Fruit salad"),
];
const MATUGEN: [(&str, &str); 9] = [
    ("scheme-tonal-spot", "Tonal spot"),
    ("scheme-vibrant", "Vibrant"),
    ("scheme-expressive", "Expressive"),
    ("scheme-neutral", "Neutral"),
    ("scheme-monochrome", "Mono"),
    ("scheme-fidelity", "Fidelity"),
    ("scheme-content", "Content"),
    ("scheme-rainbow", "Rainbow"),
    ("scheme-fruit-salad", "Fruit salad"),
];
const NOCTALIA: [(&str, &str); 10] = [
    ("m3-tonal-spot", "Tonal spot"),
    ("m3-content", "Content"),
    ("m3-fruit-salad", "Fruit salad"),
    ("m3-rainbow", "Rainbow"),
    ("m3-monochrome", "Mono"),
    ("vibrant", "Vibrant"),
    ("faithful", "Faithful"),
    ("soft", "Soft"),
    ("dysfunctional", "Dysfunc"),
    ("muted", "Muted"),
];
const WALLUST: [(&str, &str); 6] = [
    ("dark", "Dark"),
    ("dark16", "Dark 16"),
    ("harddark", "Hard dark"),
    ("softdark", "Soft dark"),
    ("light", "Light"),
    ("softlight", "Soft light"),
];
const PYWAL: [(&str, &str); 5] = [
    ("", "Natural"),
    ("0.4", "Soft"),
    ("0.6", "Balanced"),
    ("0.8", "Rich"),
    ("1.0", "Full colour"),
];
const STYLES: [(&str, &str); 4] =
    [("natural", "Natural"), ("pastel", "Pastel"), ("muted", "Muted"), ("vibrant", "Vibrant")];
const STATIC: [(&str, &str); 7] = [
    ("nord", "Nord"),
    ("dracula", "Dracula"),
    ("tokyo-night", "Tokyo Night"),
    ("catppuccin", "Catppuccin"),
    ("gruvbox", "Gruvbox"),
    ("rose-pine", "Rose Pine"),
    ("custom", "Custom"),
];

pub fn previewable_backends<'a>(available: &'a [&'a str]) -> Vec<&'a str> {
    available
        .iter()
        .copied()
        .filter(|backend| {
            matches!(
                *backend,
                "native"
                    | "static"
                    | "skwd-iris"
                    | "skwd-pywal"
                    | "skwd-wallust"
                    | "matugen"
                    | "wallust"
                    | "pywal"
                    | "iris"
                    | "noctalia"
                    | "dms"
            )
        })
        .collect()
}

fn for_backend(config: &Config, backend: &str) -> Config {
    use skwd_config::keys::theme;
    match backend {
        "static" => config.with_override(theme::POLICY, json!("fixed")),
        "noctalia" | "dms" => config
            .with_override(theme::POLICY, json!("wallpaper"))
            .with_override(theme::AUTHORITY, json!(backend)),
        engine => config
            .with_override(theme::POLICY, json!("wallpaper"))
            .with_override(theme::AUTHORITY, json!("skwd"))
            .with_override(theme::ENGINE, json!(engine)),
    }
}

fn modes(backend: &str) -> (&'static str, &'static [(&'static str, &'static str)]) {
    use skwd_config::keys::{matugen, theme};
    match backend {
        "static" => (theme::STATIC_THEME, &STATIC),
        "matugen" | "dms" => (matugen::SCHEME_TYPE, &MATUGEN),
        "noctalia" => (theme::NOCTALIA_SCHEME, &NOCTALIA),
        "wallust" | "skwd-wallust" => (theme::WALLUST_PALETTE, &WALLUST),
        "pywal" => (theme::PYWAL_SATURATE, &PYWAL),
        "skwd-pywal" => (theme::STYLE, &STYLES),
        _ => (theme::SCHEME, &MATERIAL),
    }
}

pub fn audition_profiles(config: &Config, image: &str, backend: &str) -> Vec<AuditionProfile> {
    let base = for_backend(config, backend);
    let (key, options) = modes(backend);
    if key == skwd_config::keys::theme::SCHEME {
        return super::preview_palettes(&base, image)
            .into_iter()
            .filter_map(|(value, palette)| {
                let label = options.iter().find(|(candidate, _)| *candidate == value)?.1;
                Some(AuditionProfile {
                    key: key.to_string(),
                    value,
                    label: label.to_string(),
                    palette,
                })
            })
            .collect();
    }
    options
        .iter()
        .filter_map(|(value, label)| {
            let candidate = base.with_override(key, json!(value));
            let palette = super::preview_palette(&candidate, image)?;
            Some(AuditionProfile {
                key: key.to_string(),
                value: (*value).to_string(),
                label: (*label).to_string(),
                palette,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previewable_catalog() {
        assert_eq!(
            previewable_backends(&["off", "skwd-iris", "noctalia", "caelestia", "dms"]),
            ["skwd-iris", "noctalia", "dms"]
        );
    }

    #[test]
    fn backend_profile_keys() {
        assert_eq!(modes("skwd-iris").0, skwd_config::keys::theme::SCHEME);
        assert_eq!(modes("matugen").0, skwd_config::keys::matugen::SCHEME_TYPE);
        assert_eq!(modes("wallust").0, skwd_config::keys::theme::WALLUST_PALETTE);
        assert_eq!(modes("pywal").0, skwd_config::keys::theme::PYWAL_SATURATE);
        assert_eq!(modes("noctalia").0, skwd_config::keys::theme::NOCTALIA_SCHEME);
        assert_eq!(modes("static").0, skwd_config::keys::theme::STATIC_THEME);
    }

    #[test]
    fn fixed_profiles_no_decode() {
        let config = Config::from_root(json!({
            "theme": {"policy": "fixed", "staticTheme": "nord", "mode": "dark"}
        }));
        let profiles = audition_profiles(&config, "/does/not/need/to/exist.png", "static");
        assert_eq!(profiles.len(), STATIC.len() - 1, "Custom omitted");
        assert_eq!(profiles[0].value, "nord");
        assert!(profiles.iter().all(|profile| {
            profile.key == skwd_config::keys::theme::STATIC_THEME
                && profile.palette.get("surfaceText").is_some()
        }));
    }
}
