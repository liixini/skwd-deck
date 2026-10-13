use std::collections::BTreeSet;
use std::os::unix::fs::MetadataExt;

const ANY_CHARS: char = '*';
const ANY_CHAR: char = '?';
const EXE_EXTENSION: &str = ".exe";

pub(crate) fn normalized(name: &str) -> String {
    name.rsplit(['/', '\\']).next().unwrap_or(name).trim().to_ascii_lowercase()
}

pub(crate) fn running() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let uid = unsafe { libc::getuid() };
    let mut names = BTreeSet::new();
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().parse::<u32>().is_err()
            || !entry.metadata().is_ok_and(|meta| meta.uid() == uid)
        {
            continue;
        }
        let path = entry.path();
        if let Ok(bytes) = std::fs::read(path.join("cmdline")) {
            let name =
                String::from_utf8_lossy(bytes.split(|byte| *byte == 0).next().unwrap_or_default());
            let name = name.rsplit(['/', '\\']).next().unwrap_or(&name).trim();
            if !name.is_empty() {
                names.insert(name.to_string());
            }
        }
        if let Ok(name) = std::fs::read_to_string(path.join("comm")) {
            let name = name.trim();
            if !name.is_empty() {
                names.insert(name.to_string());
            }
        }
    }
    names.into_iter().collect()
}

pub(crate) fn matches(rules: &str, running: &[String]) -> Vec<String> {
    let running: Vec<_> = running.iter().map(|name| normalized(name)).collect();
    rules
        .split([',', '\n'])
        .map(str::trim)
        .filter(|rule| !rule.is_empty())
        .filter(|rule| {
            let pattern = normalized(rule);
            let with_extension =
                pattern.contains([ANY_CHARS, ANY_CHAR]) && pattern.ends_with(EXE_EXTENSION);
            let pattern = if with_extension {
                pattern.as_str()
            } else {
                pattern.trim_end_matches(EXE_EXTENSION)
            };
            running.iter().any(|name| {
                pattern_matches(pattern, name)
                    || (!with_extension
                        && pattern_matches(pattern, name.trim_end_matches(EXE_EXTENSION)))
            })
        })
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn pattern_matches(pattern: &str, name: &str) -> bool {
    let mut matched = vec![false; name.chars().count() + 1];
    matched[0] = true;
    for token in pattern.chars() {
        let mut previous = matched[0];
        matched[0] &= token == ANY_CHARS;
        for (idx, character) in name.chars().enumerate() {
            let current = matched[idx + 1];
            matched[idx + 1] = if token == ANY_CHARS {
                current || matched[idx]
            } else {
                previous && (token == ANY_CHAR || token == character)
            };
            previous = current;
        }
    }
    matched[matched.len() - 1]
}

#[cfg(test)]
mod tests;
