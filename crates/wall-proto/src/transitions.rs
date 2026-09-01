#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Random,
    Fade,
    Wipe,
    Warp,
    Break,
    Sand,
}

impl Family {
    pub const fn key(self) -> &'static str {
        match self {
            Family::Random => "random",
            Family::Fade => "fade",
            Family::Wipe => "wipe",
            Family::Warp => "warp",
            Family::Break => "break",
            Family::Sand => "sand",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Family::Random => "Random",
            Family::Fade => "Fade",
            Family::Wipe => "Wipe",
            Family::Warp => "Warp",
            Family::Break => "Break up",
            Family::Sand => "Sand",
        }
    }

    pub const fn desc(self) -> &'static str {
        match self {
            Family::Random => "A different transition every time.",
            Family::Fade => "Dissolves and blends, no geometry.",
            Family::Wipe => "A shape or edge sweeps across the screen.",
            Family::Warp => "The image bends, stretches or ripples.",
            Family::Break => "The image shatters, pixelates or smears.",
            Family::Sand => "Every pixel becomes a grain and flies into a shape.",
        }
    }
}

pub const FAMILIES: [Family; 6] =
    [Family::Random, Family::Fade, Family::Wipe, Family::Warp, Family::Break, Family::Sand];

#[derive(Debug, Clone, Copy)]
pub struct TransitionSpec {
    pub key: &'static str,
    pub family: Family,
}

pub const TRANSITIONS: [TransitionSpec; 40] = [
    TransitionSpec { key: "random", family: Family::Random },
    TransitionSpec { key: "chromatic-bloom", family: Family::Break },
    TransitionSpec { key: "circle-crop", family: Family::Wipe },
    TransitionSpec { key: "colour-distance", family: Family::Fade },
    TransitionSpec { key: "crossfade", family: Family::Fade },
    TransitionSpec { key: "crosswarp", family: Family::Warp },
    TransitionSpec { key: "crosshatch", family: Family::Wipe },
    TransitionSpec { key: "directional", family: Family::Wipe },
    TransitionSpec { key: "directional-scaled", family: Family::Wipe },
    TransitionSpec { key: "directional-wipe", family: Family::Wipe },
    TransitionSpec { key: "fadecolor", family: Family::Fade },
    TransitionSpec { key: "glitch", family: Family::Break },
    TransitionSpec { key: "glitch-displace", family: Family::Break },
    TransitionSpec { key: "heat-melt", family: Family::Warp },
    TransitionSpec { key: "ink-splash", family: Family::Break },
    TransitionSpec { key: "inkwell-drop", family: Family::Break },
    TransitionSpec { key: "iris", family: Family::Wipe },
    TransitionSpec { key: "morph", family: Family::Warp },
    TransitionSpec { key: "mosaic-tumble", family: Family::Break },
    TransitionSpec { key: "parametric-glitch", family: Family::Break },
    TransitionSpec { key: "perlin", family: Family::Fade },
    TransitionSpec { key: "pixelate", family: Family::Break },
    TransitionSpec { key: "pixelfade-wave", family: Family::Fade },
    TransitionSpec { key: "plasma-flow", family: Family::Break },
    TransitionSpec { key: "polka-dots-curtain", family: Family::Wipe },
    TransitionSpec { key: "puzzle-right", family: Family::Wipe },
    TransitionSpec { key: "randomsquares", family: Family::Wipe },
    TransitionSpec { key: "sand-bloom", family: Family::Sand },
    TransitionSpec { key: "sand-donut", family: Family::Sand },
    TransitionSpec { key: "sand-galaxy", family: Family::Sand },
    TransitionSpec { key: "sand-globe", family: Family::Sand },
    TransitionSpec { key: "sand-helix", family: Family::Sand },
    TransitionSpec { key: "sand-maelstrom", family: Family::Sand },
    TransitionSpec { key: "sand-mobius", family: Family::Sand },
    TransitionSpec { key: "sand-murmuration", family: Family::Sand },
    TransitionSpec { key: "sand-tornado", family: Family::Sand },
    TransitionSpec { key: "smoke", family: Family::Break },
    TransitionSpec { key: "soft-warp-fade", family: Family::Fade },
    TransitionSpec { key: "voronoi-shatter", family: Family::Break },
    TransitionSpec { key: "zoom-blur-pull", family: Family::Warp },
];

pub fn family_of(key: &str) -> Family {
    if let Some(spec) = TRANSITIONS.iter().find(|spec| spec.key == key) {
        return spec.family;
    }
    if key.starts_with("sand-") {
        return Family::Sand;
    }
    Family::Break
}

pub fn keys_in_family(family: Family) -> impl Iterator<Item = &'static str> {
    TRANSITIONS.iter().filter(move |spec| spec.family == family).map(|spec| spec.key)
}

#[cfg(test)]
#[path = "transitions_tests.rs"]
mod tests;
