use std::ffi::OsStr;
use std::path::{Path, PathBuf};

// Scanners and watchers must never surface anything below this as a wallpaper.
pub const INTERNAL_LIBRARY_DIR: &str = ".skwd-wall-v2";

pub fn is_internal_library_path(path: &Path) -> bool {
    path.components().any(|component| component.as_os_str() == INTERNAL_LIBRARY_DIR)
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map_or_else(|| home().join(".local").join("share"), PathBuf::from)
}

pub fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map_or_else(|| home().join(".cache"), PathBuf::from)
        .join("skwd-wall-v2")
}

pub fn data_dir() -> PathBuf {
    data_home().join("skwd-wall-v2")
}

pub fn lens_data_dir() -> PathBuf {
    data_home().join("skwd-lens")
}

pub fn db_path() -> PathBuf {
    data_dir().join("wall.sqlite")
}

pub fn thumbs_dir() -> PathBuf {
    cache_dir().join("thumbs")
}

pub fn thumbs_sm_dir() -> PathBuf {
    cache_dir().join("thumbs-sm")
}

pub fn blocks_dir() -> PathBuf {
    cache_dir().join("blocks")
}

pub fn tmp_suffix() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    format!("{}.{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed))
}

pub fn sibling_bin(name: &str) -> PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let cand = dir.join(name);
        if cand.exists() {
            return cand;
        }
    }
    PathBuf::from(name)
}

pub fn paper_bin() -> PathBuf {
    let bin_dir = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf));
    resolve_preferred_binary(
        bin_dir.as_deref(),
        std::env::var_os("PATH").as_deref(),
        &["skwd-paper-v2"],
    )
    .unwrap_or_else(|| PathBuf::from("skwd-paper-v2"))
}

pub fn resolve_preferred_binary(
    bin_dir: Option<&Path>,
    search_path: Option<&OsStr>,
    names: &[&str],
) -> Option<PathBuf> {
    names.iter().find_map(|name| resolve_binary(bin_dir, search_path, OsStr::new(name)))
}

pub fn resolve_binary(
    bin_dir: Option<&Path>,
    search_path: Option<&OsStr>,
    name: &OsStr,
) -> Option<PathBuf> {
    bin_dir
        .map(|directory| directory.join(name))
        .filter(|candidate| is_executable(candidate))
        .or_else(|| {
            search_path.and_then(|value| {
                std::env::split_paths(value)
                    .map(|directory| directory.join(name))
                    .find(|candidate| is_executable(candidate))
            })
        })
}

#[cfg(unix)]
pub fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
pub fn is_executable(path: &Path) -> bool {
    path.is_file()
}

pub fn video_thumbs_dir() -> PathBuf {
    cache_dir().join("video-thumbs")
}

pub fn we_thumbs_dir() -> PathBuf {
    cache_dir().join("we-thumbs")
}

pub fn safe_component(comp: &str) -> bool {
    !comp.is_empty()
        && comp != "."
        && comp != ".."
        && comp.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.')
}

pub const SHIPPED_TEMPLATES: &str = "data/matugen/templates";

pub fn shipped_templates_dir() -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        roots.push(dir.join(SHIPPED_TEMPLATES));
        if let Some(up) = dir.parent() {
            roots.push(up.join("share").join("skwd-wall-v2").join(SHIPPED_TEMPLATES));
            roots.push(up.join(SHIPPED_TEMPLATES));
        }
    }
    roots.push(PathBuf::from("/usr/share/skwd-wall-v2").join(SHIPPED_TEMPLATES));
    roots.push(PathBuf::from("/usr/local/share/skwd-wall-v2").join(SHIPPED_TEMPLATES));
    roots.into_iter().find(|dir| dir.is_dir())
}

pub fn we_thumb(we_id: &str) -> PathBuf {
    we_thumbs_dir().join(format!("{we_id}.webp"))
}

pub fn we_thumb_sm(we_id: &str) -> PathBuf {
    thumbs_sm_dir().join(format!("we--{we_id}.webp"))
}

pub fn outputs_state_path() -> PathBuf {
    cache_dir().join("outputs.json")
}

pub fn previews_dir() -> PathBuf {
    cache_dir().join("previews")
}

pub fn video_preview(rel: &str) -> PathBuf {
    previews_dir().join(format!("{}.webp", thumb_name(rel)))
}

pub fn we_preview(we_id: &str) -> PathBuf {
    previews_dir().join(format!("we--{we_id}.webp"))
}

pub fn preview_for_key(key: &str) -> Option<PathBuf> {
    if let Some(rel) = key.strip_prefix("video:") {
        Some(video_preview(rel))
    } else {
        key.strip_prefix("we:").map(we_preview)
    }
}

pub fn remote_thumbs_dir() -> PathBuf {
    cache_dir().join("remote-thumbs")
}

fn segment(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|ch| if ch == '/' || ch == '\\' || ch == '\0' { '_' } else { ch })
        .collect();
    match cleaned.as_str() {
        "" | "." => "_".to_string(),
        ".." => "__".to_string(),
        _ => cleaned,
    }
}

pub fn remote_thumb(source: &str, id: &str) -> PathBuf {
    remote_thumbs_dir().join(segment(source)).join(format!("{}.webp", segment(id)))
}

pub fn remote_preview(source: &str, id: &str, ext: &str) -> PathBuf {
    cache_dir().join("remote-preview").join(segment(source)).join(format!(
        "{}.{}",
        segment(id),
        segment(ext)
    ))
}

pub fn thumb_name(rel: &str) -> String {
    let stem = rel.rsplit_once('.').map_or(rel, |(base, _)| base);
    stem.replace('/', "--")
}

pub fn image_thumb(rel: &str) -> PathBuf {
    thumbs_dir().join(format!("{}.webp", thumb_name(rel)))
}

pub fn image_thumb_sm(rel: &str) -> PathBuf {
    thumbs_sm_dir().join(format!("{}.webp", thumb_name(rel)))
}

pub fn video_thumb(rel: &str) -> PathBuf {
    video_thumbs_dir().join(format!("{}.webp", thumb_name(rel)))
}

pub fn video_thumb_sm(rel: &str) -> PathBuf {
    thumbs_sm_dir().join(format!("vid--{}.webp", thumb_name(rel)))
}

pub fn is_video_path(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| {
        paper_control::VIDEO_EXTS.iter().any(|vext| ext.eq_ignore_ascii_case(vext))
    })
}

pub fn key_for_path(path: &Path, wallpaper_dir: &str, video_dir: &str) -> Option<String> {
    let ordered = if is_video_path(path) {
        [(video_dir, wall_proto::kind::VIDEO), (wallpaper_dir, wall_proto::kind::STATIC)]
    } else {
        [(wallpaper_dir, wall_proto::kind::STATIC), (video_dir, wall_proto::kind::VIDEO)]
    };
    for (dir, kind) in ordered {
        if let Ok(rel) = path.strip_prefix(dir) {
            let rel = rel.to_string_lossy();
            if !rel.is_empty() {
                return Some(format!("{kind}:{rel}"));
            }
        }
    }
    None
}

#[path = "tests.rs"]
mod tests;
