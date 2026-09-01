use rusqlite::{Connection, OptionalExtension, params};

use super::tags::{all_effect_tags, merge_effect_tag, merge_tag, stem_key};
use crate::paths;

pub const TINIER_CONVERT_MAX_BYTES: u64 = 256 * 1024 * 1024;
pub const TINIER_CONVERT_PRESET: &str = "tinier-av1-v1";

pub fn list_wallpapers(
    conn: &Connection,
    favourite_only: bool,
) -> rusqlite::Result<Vec<serde_json::Value>> {
    let sql = if favourite_only {
        "SELECT key, name, type, thumb, thumb_sm, favourite, hue, sat, tags, colors, matugen, video_file, we_id, analyzed_by, filesize, width, height, duration_ms, mtime, weather, richness, apply_count, last_applied FROM meta WHERE favourite = 1 ORDER BY name"
    } else {
        "SELECT key, name, type, thumb, thumb_sm, favourite, hue, sat, tags, colors, matugen, video_file, we_id, analyzed_by, filesize, width, height, duration_ms, mtime, weather, richness, apply_count, last_applied FROM meta ORDER BY name"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| {
        let key = row.get::<_, Option<String>>(0)?;
        let ty = row.get::<_, Option<String>>(2)?;
        let preview = (ty.as_deref() == Some(wall_proto::kind::VIDEO))
            .then(|| key.as_deref().and_then(paths::preview_for_key))
            .flatten()
            .map(|path| path.to_string_lossy().into_owned());
        Ok(serde_json::json!({
            "key": key,
            "name": row.get::<_, Option<String>>(1)?,
            "type": ty,
            "preview": preview,
            "thumb": row.get::<_, Option<String>>(3)?,
            "thumb_sm": row.get::<_, Option<String>>(4)?,
            "favourite": row.get::<_, Option<i64>>(5)?,
            "hue": row.get::<_, Option<i64>>(6)?,
            "sat": row.get::<_, Option<i64>>(7)?,
            "tags": row.get::<_, Option<String>>(8)?,
            "colors": row.get::<_, Option<String>>(9)?,
            "matugen": row.get::<_, Option<String>>(10)?,
            "video_file": row.get::<_, Option<String>>(11)?,
            "we_id": row.get::<_, Option<String>>(12)?,
            "analyzed_by": row.get::<_, Option<String>>(13)?,
            "filesize": row.get::<_, Option<i64>>(14)?,
            "width": row.get::<_, Option<i64>>(15)?,
            "height": row.get::<_, Option<i64>>(16)?,
            "duration_ms": row.get::<_, Option<i64>>(17)?,
            "mtime": row.get::<_, Option<i64>>(18)?,
            "weather": row.get::<_, Option<String>>(19)?,
            "richness": row.get::<_, Option<i64>>(20)?,
            "apply_count": row.get::<_, Option<i64>>(21)?,
            "last_applied": row.get::<_, Option<i64>>(22)?,
        }))
    })?;
    let mut out: Vec<serde_json::Value> = rows.filter_map(std::result::Result::ok).collect();
    let effects = all_effect_tags(conn);
    if !effects.is_empty() {
        for item in &mut out {
            let key = item.get("key").and_then(serde_json::Value::as_str).map(str::to_string);
            let Some(key) = key else { continue };
            if let Some(fx) = effects.get(&stem_key(&key)) {
                let cur =
                    item.get("tags").and_then(serde_json::Value::as_str).unwrap_or("").to_string();
                item["tags"] = serde_json::Value::String(merge_tag(&cur, fx));
            }
        }
    }
    Ok(out)
}

pub fn list_wallpapers_json(
    conn: &Connection,
    favourite_only: bool,
) -> rusqlite::Result<(String, usize)> {
    let sql = if favourite_only {
        "SELECT key, name, type, thumb, thumb_sm, favourite, hue, sat, tags, colors, matugen, video_file, we_id, analyzed_by, filesize, width, height, duration_ms, mtime, weather, richness, apply_count, last_applied FROM meta WHERE favourite = 1 ORDER BY name"
    } else {
        "SELECT key, name, type, thumb, thumb_sm, favourite, hue, sat, tags, colors, matugen, video_file, we_id, analyzed_by, filesize, width, height, duration_ms, mtime, weather, richness, apply_count, last_applied FROM meta ORDER BY name"
    };
    let effects = all_effect_tags(conn);
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query([])?;
    let mut out = String::with_capacity(1 << 20);
    out.push('[');
    let mut count = 0usize;
    while let Some(row) = rows.next()? {
        let key = row.get::<_, Option<String>>(0)?;
        let ty = row.get::<_, Option<String>>(2)?;
        let preview = (ty.as_deref() == Some(wall_proto::kind::VIDEO))
            .then(|| key.as_deref().and_then(paths::preview_for_key))
            .flatten()
            .map(|path| path.to_string_lossy().into_owned());
        let mut tags = row.get::<_, Option<String>>(8)?;
        if !effects.is_empty()
            && let Some(kstr) = key.as_deref()
            && let Some(fx) = effects.get(&stem_key(kstr))
        {
            tags = Some(merge_tag(tags.as_deref().unwrap_or(""), fx));
        }
        let item = wall_proto::WallpaperItem {
            key: key.clone(),
            name: row.get(1)?,
            kind: ty,
            preview,
            thumb: row.get(3)?,
            thumb_sm: row.get(4)?,
            favourite: row.get(5)?,
            hue: row.get(6)?,
            sat: row.get(7)?,
            tags,
            review_tags: None,
            colors: row.get(9)?,
            matugen: row.get(10)?,
            video_file: row.get(11)?,
            we_id: row.get(12)?,
            analyzed_by: row.get(13)?,
            filesize: row.get(14)?,
            width: row.get(15)?,
            height: row.get(16)?,
            duration_ms: row.get(17)?,
            mtime: row.get(18)?,
            weather: row.get(19)?,
            richness: row.get(20)?,
            apply_count: row.get(21)?,
            last_applied: row.get(22)?,
        };
        if count > 0 {
            out.push(',');
        }
        match serde_json::to_string(&item) {
            Ok(json) => out.push_str(&json),
            Err(_) => out.push_str("null"),
        }
        count += 1;
    }
    out.push(']');
    Ok((out, count))
}

#[allow(clippy::too_many_arguments)]
pub fn upsert_cache_entry(
    conn: &Connection,
    key: &str,
    wp_type: &str,
    name: &str,
    thumb: &str,
    thumb_sm: &str,
    video_file: &str,
    we_id: &str,
    mtime: i64,
    hue: i64,
    sat: i64,
    richness: i64,
    filesize: i64,
    width: i64,
    height: i64,
) -> rusqlite::Result<()> {
    conn.prepare_cached(
        "INSERT INTO meta(key, type, name, thumb, thumb_sm, video_file, we_id, mtime, hue, sat, richness, filesize, width, height)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT(key) DO UPDATE SET
           type=excluded.type, name=excluded.name, thumb=excluded.thumb,
           thumb_sm=excluded.thumb_sm, video_file=excluded.video_file,
           we_id=excluded.we_id, mtime=excluded.mtime, hue=excluded.hue, sat=excluded.sat,
           richness=excluded.richness, filesize=excluded.filesize,
           width=excluded.width, height=excluded.height",
    )?
    .execute(
        params![key, wp_type, name, thumb, thumb_sm, video_file, we_id, mtime, hue, sat, richness, filesize, width, height],
    )?;
    Ok(())
}

pub fn set_favourite(conn: &Connection, key: &str, favourite: bool) -> rusqlite::Result<bool> {
    let changed = conn.execute(
        "UPDATE meta SET favourite = ?1 WHERE key = ?2",
        params![i64::from(favourite), key],
    )?;
    Ok(changed > 0)
}

pub fn update_duration(conn: &Connection, key: &str, duration_ms: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE meta SET duration_ms = ?1 WHERE key = ?2",
        params![duration_ms.max(0), key],
    )
}

pub fn clear_cache(conn: &Connection) -> rusqlite::Result<usize> {
    let transaction = conn.unchecked_transaction()?;
    transaction.execute("DELETE FROM analysis_tags", [])?;
    transaction.execute("DELETE FROM analysis_candidates", [])?;
    let changed = transaction.execute("DELETE FROM meta", [])?;
    transaction.commit()?;
    Ok(changed)
}

pub fn color_rows(conn: &Connection) -> rusqlite::Result<Vec<(String, String)>> {
    let mut stmt =
        conn.prepare("SELECT key, thumb FROM meta WHERE thumb IS NOT NULL AND thumb != ''")?;
    let rows =
        stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    Ok(rows.filter_map(std::result::Result::ok).collect())
}

pub fn thumb_for_key(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT thumb FROM meta WHERE key = ?1", params![key], |row| {
        row.get::<_, Option<String>>(0)
    })
    .optional()
    .map(Option::flatten)
}

pub fn update_colors(
    conn: &Connection,
    key: &str,
    hue: i64,
    sat: i64,
    richness: i64,
) -> rusqlite::Result<usize> {
    conn.prepare_cached("UPDATE meta SET hue = ?1, sat = ?2, richness = ?3 WHERE key = ?4")?
        .execute(params![hue, sat, richness, key])
}

pub fn update_user_tags(conn: &Connection, key: &str, tags: &str) -> rusqlite::Result<bool> {
    let tags = merge_effect_tag(conn, key, tags);
    let changed = conn.execute(
        "UPDATE meta
         SET user_tags = ?1, tags = ?1, tags_raw = ?1, manual_locked = 1
         WHERE key = ?2",
        params![tags, key],
    )?;
    Ok(changed > 0)
}

pub fn bump_apply_count(conn: &Connection, key: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE meta
         SET apply_count = COALESCE(apply_count, 0) + 1,
             last_applied = COALESCE((SELECT MAX(last_applied) FROM meta), 0) + 1
         WHERE key = ?1",
        params![key],
    )
}

pub fn key_for_video_file(conn: &Connection, video_file: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT key FROM meta WHERE video_file = ?1 LIMIT 1",
        params![video_file],
        |row| row.get(0),
    )
    .optional()
}

pub fn has_entry(conn: &Connection, key: &str) -> bool {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM meta WHERE key = ?1)", params![key], |row| {
        row.get(0)
    })
    .unwrap_or(false)
}

pub fn known_keys(conn: &Connection) -> rusqlite::Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare("SELECT key, COALESCE(mtime, 0) FROM meta")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
    Ok(rows.flatten().collect())
}

pub fn known_we_meta(conn: &Connection) -> rusqlite::Result<Vec<(String, i64, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT key, COALESCE(mtime, 0), COALESCE(type, ''), COALESCE(video_file, '') \
         FROM meta WHERE key LIKE 'we:%'",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    Ok(rows.flatten().collect())
}

pub fn item_count(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT count(*) FROM meta", [], |row| row.get(0))
}

pub fn retire_video_converts(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let destinations = {
        let mut statement = conn.prepare("SELECT src, dest FROM video_convert")?;
        let rows = statement
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
        rows.flatten()
            .filter_map(|(source, destination)| (source != destination).then_some(destination))
            .collect()
    };
    conn.execute("DELETE FROM video_convert", [])?;
    Ok(destinations)
}

pub fn tinier_convert_record(
    conn: &Connection,
    src: &str,
    dest: &str,
    frame_rate: &str,
    preset: &str,
    orig_size: i64,
    new_size: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO tinier_convert(src, dest, frame_rate, preset, orig_size, new_size, converted_at)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, strftime('%s','now'))",
        params![src, dest, frame_rate, preset, orig_size, new_size],
    )?;
    Ok(())
}

pub fn tinier_convert_entry(
    conn: &Connection,
    src: &str,
) -> rusqlite::Result<Option<(String, String, String, i64)>> {
    conn.query_row(
        "SELECT dest, frame_rate, preset, COALESCE(orig_size, 0) FROM tinier_convert WHERE src = ?1",
        params![src],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )
    .optional()
}

pub fn tinier_convert_src(conn: &Connection, dest: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT src FROM tinier_convert WHERE dest = ?1", params![dest], |row| {
        row.get(0)
    })
    .optional()
}

pub fn tinier_convert_delete(conn: &Connection, src: &str) -> rusqlite::Result<Option<String>> {
    let destination = tinier_convert_entry(conn, src)?.map(|entry| entry.0);
    conn.execute("DELETE FROM tinier_convert WHERE src = ?1", params![src])?;
    Ok(destination)
}

pub fn thumb_for_video(conn: &Connection, video_file: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT thumb FROM meta WHERE type = 'video' AND video_file = ?1 LIMIT 1",
        params![video_file],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map(Option::flatten)
}

pub fn delete_by_name(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    let changed = conn.execute("DELETE FROM meta WHERE name = ?1", params![name])?;
    Ok(changed > 0)
}

pub fn delete_entries(conn: &Connection, keys: &[String]) -> rusqlite::Result<usize> {
    if keys.is_empty() {
        return Ok(0);
    }
    let placeholders = vec!["?"; keys.len()].join(",");
    let sql = format!("DELETE FROM meta WHERE key IN ({placeholders})");
    let binds: Vec<&dyn rusqlite::types::ToSql> =
        keys.iter().map(|key| key as &dyn rusqlite::types::ToSql).collect();
    conn.execute(&sql, binds.as_slice())
}

pub fn random_pick(
    conn: &Connection,
    exclude_name: Option<&str>,
    types: &[&str],
    favourites_only: bool,
) -> rusqlite::Result<Option<(String, String, String, String, String)>> {
    if types.is_empty() {
        return Ok(None);
    }
    let placeholders = std::iter::repeat_n("?", types.len()).collect::<Vec<_>>().join(",");
    let mut sql = format!(
        "SELECT key, type, name, COALESCE(video_file,''), COALESCE(we_id,'') \
         FROM meta WHERE type IN ({placeholders})"
    );
    if favourites_only {
        sql.push_str(" AND favourite = 1");
    }
    if exclude_name.is_some() {
        sql.push_str(" AND name != ?");
    }
    sql.push_str(" ORDER BY RANDOM() LIMIT 1");

    let mut stmt = conn.prepare(&sql)?;
    let mut binds: Vec<&dyn rusqlite::ToSql> =
        types.iter().map(|ty| ty as &dyn rusqlite::ToSql).collect();
    if let Some(name) = exclude_name.as_ref() {
        binds.push(name);
    }
    stmt.query_row(rusqlite::params_from_iter(binds), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })
    .optional()
}

#[path = "wallpapers_tests.rs"]
mod tests;
