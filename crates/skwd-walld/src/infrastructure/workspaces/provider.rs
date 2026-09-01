#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Backend {
    Niri,
    Hyprland,
    Kwin,
}

fn is_niri() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .is_ok_and(|desktop| desktop.to_lowercase().contains("niri"))
        || std::env::var_os("NIRI_SOCKET").is_some()
}

pub(super) fn classify_backend(niri: bool, hyprland: bool, desktop: &str) -> Option<Backend> {
    if niri {
        return Some(Backend::Niri);
    }
    if hyprland {
        return Some(Backend::Hyprland);
    }
    let desktop = desktop.to_lowercase();
    (desktop.contains("kde") || desktop.contains("plasma")).then_some(Backend::Kwin)
}

pub(super) fn detect_backend() -> Option<Backend> {
    classify_backend(
        is_niri(),
        std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some(),
        &std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
    )
}
