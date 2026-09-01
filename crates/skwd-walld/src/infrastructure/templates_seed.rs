const TEMPLATES: [(&str, &str); 19] = [
    ("btop.theme", include_str!("../../../../data/matugen/templates/btop.theme")),
    ("foot-live.sh", include_str!("../../../../data/matugen/templates/foot-live.sh")),
    ("foot.ini", include_str!("../../../../data/matugen/templates/foot.ini")),
    ("ghostty.conf", include_str!("../../../../data/matugen/templates/ghostty.conf")),
    ("kde-colors.colors", include_str!("../../../../data/matugen/templates/kde-colors.colors")),
    ("kitty.conf", include_str!("../../../../data/matugen/templates/kitty.conf")),
    (
        "skwd-aura-colors.conf",
        include_str!("../../../../data/matugen/templates/skwd-aura-colors.conf"),
    ),
    ("niri-colors.kdl", include_str!("../../../../data/matugen/templates/niri-colors.kdl")),
    ("omp-env.sh", include_str!("../../../../data/matugen/templates/omp-env.sh")),
    ("omp.json", include_str!("../../../../data/matugen/templates/omp.json")),
    ("qt6ct-colors.conf", include_str!("../../../../data/matugen/templates/qt6ct-colors.conf")),
    (
        "quickshell-colors.json",
        include_str!("../../../../data/matugen/templates/quickshell-colors.json"),
    ),
    ("spicetify.css", include_str!("../../../../data/matugen/templates/spicetify.css")),
    ("spicetify.ini", include_str!("../../../../data/matugen/templates/spicetify.ini")),
    ("vesktop.css", include_str!("../../../../data/matugen/templates/vesktop.css")),
    ("vscode-theme.json", include_str!("../../../../data/matugen/templates/vscode-theme.json")),
    ("yazi-theme.toml", include_str!("../../../../data/matugen/templates/yazi-theme.toml")),
    ("zen-content.css", include_str!("../../../../data/matugen/templates/zen-content.css")),
    ("zen.css", include_str!("../../../../data/matugen/templates/zen.css")),
];

pub fn seed(dir: &std::path::Path) -> usize {
    if std::fs::create_dir_all(dir).is_err() {
        return 0;
    }
    let mut written = 0;
    for (name, content) in TEMPLATES {
        let dest = dir.join(name);
        if dest.exists() {
            continue;
        }
        match std::fs::write(&dest, content) {
            Ok(()) => written += 1,
            Err(err) => log::warn!("template seed: write {} failed: {err}", dest.display()),
        }
    }
    if written > 0 {
        log::info!("template seed: wrote {written} default template(s) to {}", dir.display());
    }
    written
}

mod tests;
