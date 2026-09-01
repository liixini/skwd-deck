use crate::{Rgb, ThemePalette};

pub const PRESETS: &[(&str, &str)] = &[
    ("nord", "Nord"),
    ("dracula", "Dracula"),
    ("tokyo-night", "Tokyo Night"),
    ("tokyo-night-storm", "Tokyo Night Storm"),
    ("tokyo-night-moon", "Tokyo Night Moon"),
    ("catppuccin", "Catppuccin"),
    ("catppuccin-macchiato", "Catppuccin Macchiato"),
    ("catppuccin-frappe", "Catppuccin Frapp\u{e9}"),
    ("catppuccin-latte", "Catppuccin Latte"),
    ("gruvbox", "Gruvbox"),
    ("rose-pine", "Rose Pine"),
    ("rose-pine-moon", "Rose Pine Moon"),
    ("rose-pine-dawn", "Rose Pine Dawn"),
    ("everforest", "Everforest"),
    ("kanagawa", "Kanagawa"),
    ("solarized-dark", "Solarized Dark"),
    ("solarized-light", "Solarized Light"),
    ("one-dark", "One Dark"),
    ("monokai", "Monokai"),
    ("github-dark", "GitHub Dark"),
    ("github-light", "GitHub Light"),
    ("night-owl", "Night Owl"),
    ("palenight", "Palenight"),
    ("synthwave84", "Synthwave '84"),
    ("ayu-dark", "Ayu Dark"),
    ("ayu-mirage", "Ayu Mirage"),
];

const fn rgb(hex: u32) -> Rgb {
    Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub fn preset(name: &str) -> Option<ThemePalette> {
    let palette = |background,
                   surface,
                   variant,
                   container,
                   on_surface,
                   primary,
                   on_primary,
                   outline,
                   tertiary| {
        ThemePalette {
            background: rgb(background),
            surface: rgb(surface),
            surface_variant: rgb(variant),
            surface_container: rgb(container),
            on_surface: rgb(on_surface),
            primary: rgb(primary),
            on_primary: rgb(on_primary),
            outline: rgb(outline),
            tertiary: rgb(tertiary),
        }
    };
    Some(match name {
        "nord" => palette(
            0x2e3440, 0x3b4252, 0x434c5e, 0x4c566a, 0xeceff4, 0x88c0d0, 0x2e3440, 0x4c566a,
            0xb48ead,
        ),
        "dracula" => palette(
            0x282a36, 0x343746, 0x44475a, 0x414458, 0xf8f8f2, 0xbd93f9, 0x282a36, 0x6272a4,
            0xff79c6,
        ),
        "tokyo-night" => palette(
            0x1a1b26, 0x24283b, 0x414868, 0x2f3549, 0xc0caf5, 0x7aa2f7, 0x1a1b26, 0x565f89,
            0xbb9af7,
        ),
        "catppuccin" => palette(
            0x1e1e2e, 0x313244, 0x45475a, 0x292c3c, 0xcdd6f4, 0x89b4fa, 0x1e1e2e, 0x6c7086,
            0xf5c2e7,
        ),
        "gruvbox" => palette(
            0x282828, 0x3c3836, 0x504945, 0x32302f, 0xebdbb2, 0xfabd2f, 0x282828, 0x665c54,
            0xfe8019,
        ),
        "rose-pine" => palette(
            0x191724, 0x1f1d2e, 0x26233a, 0x21202e, 0xe0def4, 0xebbcba, 0x191724, 0x524f67,
            0xc4a7e7,
        ),
        "everforest" => palette(
            0x2d353b, 0x343f44, 0x3d484d, 0x475258, 0xd3c6aa, 0xa7c080, 0x2d353b, 0x859289,
            0x83c092,
        ),
        "tokyo-night-storm" => palette(
            0x1f2335, 0x24283b, 0x292e42, 0x414868, 0xc0caf5, 0x7aa2f7, 0x1f2335, 0x565f89,
            0xbb9af7,
        ),
        "tokyo-night-moon" => palette(
            0x1e2030, 0x222436, 0x2f334d, 0x444a73, 0xc8d3f5, 0x82aaff, 0x1e2030, 0x636da6,
            0xc099ff,
        ),
        "catppuccin-macchiato" => palette(
            0x181926, 0x24273a, 0x363a4f, 0x494d64, 0xcad3f5, 0xc6a0f6, 0x181926, 0x6e738d,
            0xf5bde6,
        ),
        "catppuccin-frappe" => palette(
            0x232634, 0x303446, 0x414559, 0x51576d, 0xc6d0f5, 0xca9ee6, 0x232634, 0x737994,
            0xf4b8e4,
        ),
        "catppuccin-latte" => palette(
            0xeff1f5, 0xe6e9ef, 0xccd0da, 0xdce0e8, 0x4c4f69, 0x8839ef, 0xffffff, 0x9ca0b0,
            0xea76cb,
        ),
        "rose-pine-moon" => palette(
            0x232136, 0x2a273f, 0x393552, 0x44415a, 0xe0def4, 0xc4a7e7, 0x232136, 0x6e6a86,
            0xea9a97,
        ),
        "rose-pine-dawn" => palette(
            0xfaf4ed, 0xfffaf3, 0xf2e9e1, 0xdfdad9, 0x575279, 0x907aa9, 0xfaf4ed, 0x9893a5,
            0xd7827e,
        ),
        "kanagawa" => palette(
            0x16161d, 0x1f1f28, 0x2a2a37, 0x363646, 0xdcd7ba, 0x7e9cd8, 0x16161d, 0x727169,
            0xd27e99,
        ),
        "solarized-dark" => palette(
            0x002b36, 0x073642, 0x0a4652, 0x11525f, 0x93a1a1, 0x268bd2, 0xfdf6e3, 0x586e75,
            0x2aa198,
        ),
        "solarized-light" => palette(
            0xfdf6e3, 0xeee8d5, 0xe4ddc4, 0xd9d2b8, 0x657b83, 0x268bd2, 0xfdf6e3, 0x93a1a1,
            0xb58900,
        ),
        "one-dark" => palette(
            0x21252b, 0x282c34, 0x2c313a, 0x333842, 0xabb2bf, 0x61afef, 0x21252b, 0x5c6370,
            0xc678dd,
        ),
        "monokai" => palette(
            0x1e1f1c, 0x272822, 0x34352f, 0x3e3d32, 0xf8f8f2, 0xf92672, 0x1e1f1c, 0x75715e,
            0x66d9ef,
        ),
        "github-dark" => palette(
            0x0d1117, 0x161b22, 0x21262d, 0x30363d, 0xc9d1d9, 0x58a6ff, 0x0d1117, 0x484f58,
            0xbc8cff,
        ),
        "github-light" => palette(
            0xffffff, 0xf6f8fa, 0xeaeef2, 0xd0d7de, 0x24292f, 0x0969da, 0xffffff, 0x8c959f,
            0x8250df,
        ),
        "night-owl" => palette(
            0x011627, 0x0b2942, 0x1d3b53, 0x234d70, 0xd6deeb, 0x82aaff, 0x011627, 0x5f7e97,
            0xc792ea,
        ),
        "palenight" => palette(
            0x292d3e, 0x32364a, 0x3a3f58, 0x444867, 0xa6accd, 0xc792ea, 0x292d3e, 0x676e95,
            0x89ddff,
        ),
        "synthwave84" => palette(
            0x241b2f, 0x262335, 0x2f2745, 0x3a2f56, 0xf0eff1, 0xff7edb, 0x241b2f, 0x848bbd,
            0x36f9f6,
        ),
        "ayu-dark" => palette(
            0x0b0e14, 0x0f131a, 0x161b24, 0x1e232d, 0xbfbdb6, 0xe6b450, 0x0b0e14, 0x565b66,
            0x59c2ff,
        ),
        "ayu-mirage" => palette(
            0x171b24, 0x1f2430, 0x242936, 0x2d3444, 0xcccac2, 0xffcc66, 0x171b24, 0x707a8c,
            0x5ccfe6,
        ),
        _ => return None,
    })
}

#[cfg(test)]
#[path = "presets_tests.rs"]
mod tests;
