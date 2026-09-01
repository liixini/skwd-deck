use std::path::Path;

pub fn bin_from(configured: &str, local_candidate: &Path, default_bin: &str) -> String {
    if !configured.is_empty() {
        return configured.to_string();
    }
    if local_candidate.is_file() {
        return local_candidate.to_string_lossy().into_owned();
    }
    default_bin.to_string()
}
