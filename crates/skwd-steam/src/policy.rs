pub fn numeric_ids(args: &[String]) -> Vec<String> {
    args.iter()
        .filter(|arg| !arg.is_empty() && arg.chars().all(|character| character.is_ascii_digit()))
        .cloned()
        .collect()
}

pub fn install_complete(installed: bool, downloading: bool, needs_update: bool) -> bool {
    installed && !downloading && !needs_update
}

pub fn download_active(downloading: bool, pending: bool, total: u64) -> bool {
    downloading || pending || total > 0
}

pub fn progress_fraction(current: u64, total: u64) -> f64 {
    if total > 0 { (current as f64 / total as f64).clamp(0.0, 1.0) } else { 0.0 }
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
