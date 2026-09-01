use std::collections::HashSet;
use std::hash::BuildHasher;

use rusqlite::{Connection, params};

pub fn stem_key(meta_key: &str) -> String {
    let rel = meta_key.split_once(':').map_or(meta_key, |(_, rest)| rest);
    match rel.rsplit_once('.') {
        Some((base, ext)) if !base.is_empty() && !ext.contains('/') => base.to_string(),
        _ => rel.to_string(),
    }
}

pub fn set_effect_tag(conn: &Connection, stem: &str, tag: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO effect_tags(stem, tag) VALUES(?1, ?2)
         ON CONFLICT(stem) DO UPDATE SET tag=excluded.tag",
        params![stem, tag],
    )?;
    Ok(())
}

pub fn effect_tag(conn: &Connection, stem: &str) -> Option<String> {
    conn.query_row("SELECT tag FROM effect_tags WHERE stem=?1", params![stem], |row| {
        row.get::<_, String>(0)
    })
    .ok()
}

pub(super) fn all_effect_tags(conn: &Connection) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT stem, tag FROM effect_tags")
        && let Ok(rows) =
            stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
    {
        for row in rows.flatten() {
            map.insert(row.0, row.1);
        }
    }
    map
}

fn tag_present(tags: &str, effect_lc: &str) -> bool {
    let trimmed = tags.trim();
    if trimmed.starts_with('[')
        && let Ok(serde_json::Value::Array(arr)) =
            serde_json::from_str::<serde_json::Value>(trimmed)
    {
        return arr
            .iter()
            .filter_map(serde_json::Value::as_str)
            .any(|tag| tag.trim().to_lowercase() == effect_lc);
    }
    trimmed.split(',').any(|tag| tag.trim().to_lowercase() == effect_lc)
}

pub fn merge_tag(tags: &str, effect: &str) -> String {
    if effect.is_empty() {
        return tags.to_string();
    }
    if tag_present(tags, &effect.to_lowercase()) {
        return tags.to_string();
    }
    let trimmed = tags.trim();
    if trimmed.is_empty() {
        effect.to_string()
    } else if trimmed.starts_with('[') {
        if let Ok(serde_json::Value::Array(mut arr)) =
            serde_json::from_str::<serde_json::Value>(trimmed)
        {
            arr.insert(0, serde_json::Value::String(effect.to_string()));
            return serde_json::Value::Array(arr).to_string();
        }
        format!("{effect},{tags}")
    } else {
        format!("{effect},{tags}")
    }
}

pub(super) fn merge_effect_tag(conn: &Connection, key: &str, tags: &str) -> String {
    match effect_tag(conn, &stem_key(key)) {
        Some(fx) => merge_tag(tags, &fx),
        None => tags.to_string(),
    }
}

pub fn parse_effect_tag<S: BuildHasher>(
    file_stem: &str,
    effect_ids: &HashSet<String, S>,
) -> Option<String> {
    let segs: Vec<&str> = file_stem.split('-').collect();
    if let Some(last) = segs.last()
        && effect_ids.contains(*last)
    {
        return Some((*last).to_string());
    }
    if let Some(idx) = file_stem.rfind("-theme-") {
        let theme = &file_stem[idx + "-theme-".len()..];
        if !theme.is_empty() {
            return Some(theme.to_string());
        }
    }
    if segs.len() >= 2
        && let Some(fx) = segs.get(segs.len() - 2)
        && effect_ids.contains(*fx)
    {
        return Some((*fx).to_string());
    }
    None
}

pub fn backfill_effect_tags<S: BuildHasher>(
    conn: &Connection,
    effect_ids: &HashSet<String, S>,
) -> rusqlite::Result<usize> {
    let rows: Vec<(String, String)> = {
        let mut stmt = conn.prepare("SELECT key, name FROM meta WHERE type='static'")?;
        let mapped =
            stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
        mapped.filter_map(std::result::Result::ok).collect()
    };
    let mut count = 0;
    for (key, name) in rows {
        let Some((dir, file)) = name.rsplit_once('/') else { continue };
        if dir.rsplit('/').next().unwrap_or(dir) != "effects" {
            continue;
        }
        let file_stem = file.rsplit_once('.').map_or(file, |(base, _)| base);
        let Some(tag) = parse_effect_tag(file_stem, effect_ids) else { continue };
        if effect_tag(conn, &stem_key(&key)).is_some() {
            continue;
        }
        set_effect_tag(conn, &stem_key(&key), &tag)?;
        count += 1;
    }
    Ok(count)
}

#[path = "tags_tests.rs"]
mod tests;
