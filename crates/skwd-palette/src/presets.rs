use crate::{Rgb, ThemePalette};

struct Preset {
    key: &'static str,
    family: &'static str,
    label: &'static str,
    colors: [u32; 9],
}

const TABLE: [Preset; 34] = [
    Preset {
        key: "nord",
        family: "nord",
        label: "Nord",
        colors: [
            0x2e3440, 0x3b4252, 0x434c5e, 0x4c566a, 0xeceff4, 0x88c0d0, 0x2e3440, 0x4c566a,
            0xb48ead,
        ],
    },
    Preset {
        key: "dracula",
        family: "dracula",
        label: "Dracula",
        colors: [
            0x282a36, 0x343746, 0x44475a, 0x414458, 0xf8f8f2, 0xbd93f9, 0x282a36, 0x6272a4,
            0xff79c6,
        ],
    },
    Preset {
        key: "tokyo-night",
        family: "tokyo-night",
        label: "Tokyo Night",
        colors: [
            0x1a1b26, 0x24283b, 0x414868, 0x2f3549, 0xc0caf5, 0x7aa2f7, 0x1a1b26, 0x565f89,
            0xbb9af7,
        ],
    },
    Preset {
        key: "tokyo-night-storm",
        family: "tokyo-night",
        label: "Tokyo Night Storm",
        colors: [
            0x1f2335, 0x24283b, 0x292e42, 0x414868, 0xc0caf5, 0x7aa2f7, 0x1f2335, 0x565f89,
            0xbb9af7,
        ],
    },
    Preset {
        key: "tokyo-night-moon",
        family: "tokyo-night",
        label: "Tokyo Night Moon",
        colors: [
            0x1e2030, 0x222436, 0x2f334d, 0x444a73, 0xc8d3f5, 0x82aaff, 0x1e2030, 0x636da6,
            0xc099ff,
        ],
    },
    Preset {
        key: "catppuccin",
        family: "catppuccin",
        label: "Catppuccin",
        colors: [
            0x1e1e2e, 0x313244, 0x45475a, 0x292c3c, 0xcdd6f4, 0x89b4fa, 0x1e1e2e, 0x6c7086,
            0xf5c2e7,
        ],
    },
    Preset {
        key: "catppuccin-macchiato",
        family: "catppuccin",
        label: "Catppuccin Macchiato",
        colors: [
            0x181926, 0x24273a, 0x363a4f, 0x494d64, 0xcad3f5, 0xc6a0f6, 0x181926, 0x6e738d,
            0xf5bde6,
        ],
    },
    Preset {
        key: "catppuccin-frappe",
        family: "catppuccin",
        label: "Catppuccin Frapp\u{e9}",
        colors: [
            0x232634, 0x303446, 0x414559, 0x51576d, 0xc6d0f5, 0xca9ee6, 0x232634, 0x737994,
            0xf4b8e4,
        ],
    },
    Preset {
        key: "catppuccin-latte",
        family: "catppuccin",
        label: "Catppuccin Latte",
        colors: [
            0xeff1f5, 0xe6e9ef, 0xccd0da, 0xdce0e8, 0x4c4f69, 0x8839ef, 0xffffff, 0x9ca0b0,
            0xea76cb,
        ],
    },
    Preset {
        key: "gruvbox",
        family: "gruvbox",
        label: "Gruvbox",
        colors: [
            0x282828, 0x3c3836, 0x504945, 0x32302f, 0xebdbb2, 0xfabd2f, 0x282828, 0x665c54,
            0xfe8019,
        ],
    },
    Preset {
        key: "rose-pine",
        family: "rose-pine",
        label: "Rose Pine",
        colors: [
            0x191724, 0x1f1d2e, 0x26233a, 0x21202e, 0xe0def4, 0xebbcba, 0x191724, 0x524f67,
            0xc4a7e7,
        ],
    },
    Preset {
        key: "rose-pine-moon",
        family: "rose-pine",
        label: "Rose Pine Moon",
        colors: [
            0x232136, 0x2a273f, 0x393552, 0x44415a, 0xe0def4, 0xc4a7e7, 0x232136, 0x6e6a86,
            0xea9a97,
        ],
    },
    Preset {
        key: "rose-pine-dawn",
        family: "rose-pine",
        label: "Rose Pine Dawn",
        colors: [
            0xfaf4ed, 0xfffaf3, 0xf2e9e1, 0xdfdad9, 0x575279, 0x907aa9, 0xfaf4ed, 0x9893a5,
            0xd7827e,
        ],
    },
    Preset {
        key: "everforest",
        family: "everforest",
        label: "Everforest",
        colors: [
            0x2d353b, 0x343f44, 0x3d484d, 0x475258, 0xd3c6aa, 0xa7c080, 0x2d353b, 0x859289,
            0x83c092,
        ],
    },
    Preset {
        key: "kanagawa",
        family: "kanagawa",
        label: "Kanagawa",
        colors: [
            0x16161d, 0x1f1f28, 0x2a2a37, 0x363646, 0xdcd7ba, 0x7e9cd8, 0x16161d, 0x727169,
            0xd27e99,
        ],
    },
    Preset {
        key: "solarized-dark",
        family: "solarized",
        label: "Solarized Dark",
        colors: [
            0x002b36, 0x073642, 0x0a4652, 0x11525f, 0x93a1a1, 0x268bd2, 0xfdf6e3, 0x586e75,
            0x2aa198,
        ],
    },
    Preset {
        key: "solarized-light",
        family: "solarized",
        label: "Solarized Light",
        colors: [
            0xfdf6e3, 0xeee8d5, 0xe4ddc4, 0xd9d2b8, 0x657b83, 0x268bd2, 0xfdf6e3, 0x93a1a1,
            0xb58900,
        ],
    },
    Preset {
        key: "one-dark",
        family: "one-dark",
        label: "One Dark",
        colors: [
            0x21252b, 0x282c34, 0x2c313a, 0x333842, 0xabb2bf, 0x61afef, 0x21252b, 0x5c6370,
            0xc678dd,
        ],
    },
    Preset {
        key: "monokai",
        family: "monokai",
        label: "Monokai",
        colors: [
            0x1e1f1c, 0x272822, 0x34352f, 0x3e3d32, 0xf8f8f2, 0xf92672, 0x1e1f1c, 0x75715e,
            0x66d9ef,
        ],
    },
    Preset {
        key: "github-dark",
        family: "github",
        label: "GitHub Dark",
        colors: [
            0x0d1117, 0x161b22, 0x21262d, 0x30363d, 0xc9d1d9, 0x58a6ff, 0x0d1117, 0x484f58,
            0xbc8cff,
        ],
    },
    Preset {
        key: "github-light",
        family: "github",
        label: "GitHub Light",
        colors: [
            0xffffff, 0xf6f8fa, 0xeaeef2, 0xd0d7de, 0x24292f, 0x0969da, 0xffffff, 0x8c959f,
            0x8250df,
        ],
    },
    Preset {
        key: "night-owl",
        family: "night-owl",
        label: "Night Owl",
        colors: [
            0x011627, 0x0b2942, 0x1d3b53, 0x234d70, 0xd6deeb, 0x82aaff, 0x011627, 0x5f7e97,
            0xc792ea,
        ],
    },
    Preset {
        key: "palenight",
        family: "palenight",
        label: "Palenight",
        colors: [
            0x292d3e, 0x32364a, 0x3a3f58, 0x444867, 0xa6accd, 0xc792ea, 0x292d3e, 0x676e95,
            0x89ddff,
        ],
    },
    Preset {
        key: "synthwave84",
        family: "synthwave84",
        label: "Synthwave '84",
        colors: [
            0x241b2f, 0x262335, 0x2f2745, 0x3a2f56, 0xf0eff1, 0xff7edb, 0x241b2f, 0x848bbd,
            0x36f9f6,
        ],
    },
    Preset {
        key: "ayu-dark",
        family: "ayu",
        label: "Ayu Dark",
        colors: [
            0x0b0e14, 0x0f131a, 0x161b24, 0x1e232d, 0xbfbdb6, 0xe6b450, 0x0b0e14, 0x565b66,
            0x59c2ff,
        ],
    },
    Preset {
        key: "ayu-mirage",
        family: "ayu",
        label: "Ayu Mirage",
        colors: [
            0x171b24, 0x1f2430, 0x242936, 0x2d3444, 0xcccac2, 0xffcc66, 0x171b24, 0x707a8c,
            0x5ccfe6,
        ],
    },
    Preset {
        key: "copper",
        family: "copper",
        label: "Copper",
        colors: [
            0x17_0c_07, 0x22_15_0e, 0x32_21_19, 0x2a_1b_13, 0xf4_e4_dd, 0xd7_7e_52, 0x2d_12_05,
            0xa5_8c_81, 0x7a_ba_a8,
        ],
    },
    Preset {
        key: "copper-light",
        family: "copper",
        label: "Copper Light",
        colors: [
            0xfb_f1_ed, 0xf3_e7_e1, 0xe8_d9_d2, 0xed_e0_d9, 0x2b_1b_13, 0x99_49_1e, 0xff_f7_f3,
            0x75_5d_53, 0x21_62_53,
        ],
    },
    Preset {
        key: "brass",
        family: "brass",
        label: "Brass",
        colors: [
            0x15_13_0c, 0x1f_1c_12, 0x2b_27_19, 0x25_21_15, 0xf0_ea_d6, 0xd8_b4_5a, 0x24_1c_05,
            0xa3_9a_7c, 0xcb_b3_99,
        ],
    },
    Preset {
        key: "brass-light",
        family: "brass",
        label: "Brass Light",
        colors: [
            0xf6_f2_e6, 0xee_e8_d6, 0xe4_dc_c5, 0xe9_e2_cd, 0x27_22_0f, 0x7d_61_10, 0xff_fb_ea,
            0x74_69_4a, 0x70_59_3e,
        ],
    },
    Preset {
        key: "verdigris",
        family: "verdigris",
        label: "Verdigris",
        colors: [
            0x0d_14_13, 0x14_1d_1b, 0x1d_29_26, 0x18_23_20, 0xe1_ef_e9, 0x6f_c7_ad, 0x05_20_19,
            0x86_a0_98, 0xdd_8e_68,
        ],
    },
    Preset {
        key: "verdigris-light",
        family: "verdigris",
        label: "Verdigris Light",
        colors: [
            0xee_f5_f2, 0xe3_ee_ea, 0xd6_e5_df, 0xdc_e9_e4, 0x13_23_1e, 0x1f_7a_63, 0xf2_ff_fa,
            0x57_71_6a, 0x95_4c_27,
        ],
    },
    Preset {
        key: "graphite",
        family: "graphite",
        label: "Graphite",
        colors: [
            0x12_13_14, 0x1a_1b_1d, 0x25_27_2a, 0x1f_21_23, 0xe8_e9_eb, 0xc9_ce_d6, 0x15_17_1a,
            0x8e_91_96, 0xb4_b9_c1,
        ],
    },
    Preset {
        key: "graphite-light",
        family: "graphite",
        label: "Graphite Light",
        colors: [
            0xf4_f4_f5, 0xeb_ec_ed, 0xdf_e0_e3, 0xe5_e6_e8, 0x1c_1d_1f, 0x3d_42_49, 0xf7_f8_f9,
            0x67_69_6e, 0x6e_73_7b,
        ],
    },
];

pub const PRESETS: [(&str, &str); TABLE.len()] = {
    let mut out = [("", ""); TABLE.len()];
    let mut index = 0;
    while index < TABLE.len() {
        out[index] = (TABLE[index].key, TABLE[index].label);
        index += 1;
    }
    out
};

const LIGHT_SUFFIX: &str = "-light";

const fn rgb(hex: u32) -> Rgb {
    Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub fn family(name: &str) -> Option<&'static str> {
    TABLE.iter().find(|preset| preset.key == name).map(|preset| preset.family)
}

pub fn light_twin(name: &str) -> Option<&'static str> {
    family(name)?;
    TABLE
        .iter()
        .find(|preset| preset.key.strip_suffix(LIGHT_SUFFIX) == Some(name))
        .map(|preset| preset.key)
}

pub fn is_light_twin(name: &str) -> bool {
    name.strip_suffix(LIGHT_SUFFIX).is_some_and(|base| light_twin(base) == Some(name))
}

pub fn relatives(name: &str) -> impl Iterator<Item = &'static str> {
    let family = family(name);
    TABLE
        .iter()
        .filter(move |preset| Some(preset.family) == family && preset.key != name)
        .map(|preset| preset.key)
}

pub fn preset(name: &str) -> Option<ThemePalette> {
    let [
        background,
        surface,
        variant,
        container,
        on_surface,
        primary,
        on_primary,
        outline,
        tertiary,
    ] = TABLE.iter().find(|preset| preset.key == name)?.colors;
    Some(ThemePalette {
        background: rgb(background),
        surface: rgb(surface),
        surface_variant: rgb(variant),
        surface_container: rgb(container),
        on_surface: rgb(on_surface),
        primary: rgb(primary),
        on_primary: rgb(on_primary),
        outline: rgb(outline),
        tertiary: rgb(tertiary),
    })
}

#[cfg(test)]
#[path = "presets_tests.rs"]
mod tests;
