use rusqlite::{Connection, params};

use super::tags::{set_effect_tag, stem_key};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct LibraryImport {
    pub matched: usize,
    pub missing: usize,
    pub effects: usize,
}

pub fn export_library(conn: &Connection) -> rusqlite::Result<String> {
    let mut out = String::new();
    {
        let mut stmt = conn.prepare(
            "SELECT key, COALESCE(favourite, 0), tags, COALESCE(we_id, '') FROM meta \
             WHERE favourite = 1 OR (tags IS NOT NULL AND tags != '') ORDER BY key",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (key, fav, tags, we_id) = row?;
            let mut obj = serde_json::Map::new();
            obj.insert("key".into(), serde_json::Value::String(key));
            obj.insert("fav".into(), serde_json::Value::Bool(fav != 0));
            if let Some(tags) = tags.filter(|text| !text.is_empty()) {
                obj.insert("tags".into(), serde_json::Value::String(tags));
            }
            if !we_id.is_empty() {
                obj.insert("we_id".into(), serde_json::Value::String(we_id));
            }
            out.push_str(&serde_json::Value::Object(obj).to_string());
            out.push('\n');
        }
    }
    {
        let mut stmt = conn.prepare("SELECT stem, tag FROM effect_tags ORDER BY stem")?;
        let rows =
            stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
        for row in rows {
            let (stem, tag) = row?;
            out.push_str(&serde_json::json!({"stem": stem, "effect": tag}).to_string());
            out.push('\n');
        }
    }
    Ok(out)
}

fn import_meta_entry(
    conn: &Connection,
    key: &str,
    fav: bool,
    tags: Option<&str>,
    we_id: &str,
) -> rusqlite::Result<bool> {
    let update = |target: &str| {
        conn.execute(
            "UPDATE meta SET favourite = ?1, tags = COALESCE(?2, tags), \
             tags_raw = COALESCE(?2, tags_raw) WHERE key = ?3",
            params![i64::from(fav), tags, target],
        )
    };
    if update(key)? > 0 {
        return Ok(true);
    }
    if !we_id.is_empty()
        && conn.execute(
            "UPDATE meta SET favourite = ?1, tags = COALESCE(?2, tags), \
             tags_raw = COALESCE(?2, tags_raw) WHERE we_id = ?3",
            params![i64::from(fav), tags, we_id],
        )? > 0
    {
        return Ok(true);
    }
    let target_stem = stem_key(key);
    let candidate = {
        let mut stmt = conn.prepare("SELECT key FROM meta")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.flatten().find(|mkey| stem_key(mkey) == target_stem)
    };
    if let Some(mk) = candidate {
        return Ok(update(&mk)? > 0);
    }
    Ok(false)
}

pub fn import_library(conn: &Connection, jsonl: &str) -> rusqlite::Result<LibraryImport> {
    let mut stats = LibraryImport::default();
    for line in jsonl.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(key) = val.get("key").and_then(serde_json::Value::as_str) {
            let fav = val.get("fav").and_then(serde_json::Value::as_bool).unwrap_or(false);
            let tags =
                val.get("tags").and_then(serde_json::Value::as_str).filter(|text| !text.is_empty());
            let we_id = val.get("we_id").and_then(serde_json::Value::as_str).unwrap_or("");
            if import_meta_entry(conn, key, fav, tags, we_id)? {
                stats.matched += 1;
            } else {
                stats.missing += 1;
            }
        } else if let Some(stem) = val.get("stem").and_then(serde_json::Value::as_str) {
            let effect = val.get("effect").and_then(serde_json::Value::as_str).unwrap_or("");
            if !effect.is_empty() {
                set_effect_tag(conn, stem, effect)?;
                stats.effects += 1;
            }
        }
    }
    Ok(stats)
}

#[path = "library_tests.rs"]
mod tests;
