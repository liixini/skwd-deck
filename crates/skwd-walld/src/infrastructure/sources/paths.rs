pub fn ext_from_url(url: &str) -> &str {
    url.split(['?', '#'])
        .next()
        .unwrap_or(url)
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, extension)| extension)
        .filter(|extension| {
            extension.len() <= 5 && extension.chars().all(|ch| ch.is_ascii_alphanumeric())
        })
        .unwrap_or("jpg")
}

pub(crate) fn safe_seg(segment: &str) -> String {
    segment.replace(['/', '\\', '\0'], "_")
}

pub fn library_path(directory: &str, source: &str, id: &str) -> Option<std::path::PathBuf> {
    let prefix = format!("{source}-{}.", safe_seg(id));
    std::fs::read_dir(directory).ok()?.filter_map(Result::ok).find_map(|entry| {
        entry.file_name().to_string_lossy().starts_with(&prefix).then(|| entry.path())
    })
}

pub fn library_ids(directory: &str, source: &str) -> std::collections::HashSet<String> {
    let mut ids = std::collections::HashSet::new();
    let prefix = format!("{source}-");
    if let Ok(entries) = std::fs::read_dir(directory) {
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            if let Some(rest) = name.to_string_lossy().strip_prefix(&prefix) {
                let id = rest.rsplit_once('.').map_or(rest, |(stem, _)| stem).to_string();
                if !id.is_empty() {
                    ids.insert(id);
                }
            }
        }
    }
    ids
}
