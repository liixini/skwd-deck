pub const FOLDER: &str = "folder:";
pub const ALL: &str = "all";
pub const FAVOURITES: &str = "favourites";
const QUOTE: char = '"';
const ESCAPE: char = '\\';

pub fn clauses(source: &str) -> Vec<String> {
    let mut clauses = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for letter in source.chars() {
        if escaped {
            current.push(letter);
            escaped = false;
        } else if quoted && letter == ESCAPE {
            current.push(letter);
            escaped = true;
        } else if letter == QUOTE {
            current.push(letter);
            quoted = !quoted;
        } else if letter.is_whitespace() && !quoted {
            if !current.is_empty() {
                clauses.push(std::mem::take(&mut current));
            }
        } else {
            current.push(letter);
        }
    }
    if !current.is_empty() {
        clauses.push(current);
    }
    clauses
}

pub fn folder_path(clause: &str) -> Option<String> {
    let value = clause.strip_prefix(FOLDER)?;
    let Some(inner) = value.strip_prefix(QUOTE).and_then(|rest| rest.strip_suffix(QUOTE)) else {
        return Some(value.to_owned());
    };
    let mut path = String::with_capacity(inner.len());
    let mut escaped = false;
    for letter in inner.chars() {
        if escaped || letter != ESCAPE {
            path.push(letter);
            escaped = false;
        } else {
            escaped = true;
        }
    }
    Some(path)
}

pub fn folder_clause(path: &str) -> String {
    if !path.chars().any(|letter| letter.is_whitespace() || letter == QUOTE || letter == ESCAPE) {
        return format!("{FOLDER}{path}");
    }
    let mut escaped = String::with_capacity(path.len());
    for letter in path.chars() {
        if letter == QUOTE || letter == ESCAPE {
            escaped.push(ESCAPE);
        }
        escaped.push(letter);
    }
    format!("{FOLDER}{QUOTE}{escaped}{QUOTE}")
}

#[cfg(test)]
#[path = "playlist_source_tests.rs"]
mod tests;
