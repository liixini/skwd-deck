use rusqlite::{Connection, OptionalExtension, params};

pub fn playlists_all(conn: &Connection) -> rusqlite::Result<Vec<wall_proto::PlaylistRow>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, p.kind, p.source, p.order_mode, p.dwell, p.position,
                (SELECT COUNT(*) FROM playlist_members m WHERE m.playlist_id = p.id)
         FROM playlists p ORDER BY p.position, p.id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(wall_proto::PlaylistRow {
            id: row.get(0)?,
            name: row.get(1)?,
            kind: row.get(2)?,
            source: row.get(3)?,
            order: row.get(4)?,
            dwell: row.get(5)?,
            position: row.get(6)?,
            count: row.get(7)?,
        })
    })?;
    rows.collect()
}

pub fn playlist_create(conn: &Connection, name: &str) -> rusqlite::Result<i64> {
    let pos: i64 =
        conn.query_row("SELECT COALESCE(MAX(position), -1) + 1 FROM playlists", [], |row| {
            row.get(0)
        })?;
    conn.execute(
        "INSERT INTO playlists(name, kind, order_mode, dwell, position) VALUES(?1, 'curated', 'shuffle', 600, ?2)",
        params![name, pos],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn playlist_delete(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM playlist_members WHERE playlist_id = ?1", params![id])?;
    conn.execute("DELETE FROM playlist_assign WHERE playlist_id = ?1", params![id])?;
    conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn playlist_update(
    conn: &Connection,
    id: i64,
    name: Option<&str>,
    kind: Option<&str>,
    source: Option<&str>,
    order: Option<&str>,
    dwell: Option<i64>,
) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE playlists SET name=COALESCE(?2,name), kind=COALESCE(?3,kind), source=COALESCE(?4,source), order_mode=COALESCE(?5,order_mode), dwell=COALESCE(?6,dwell) WHERE id=?1",
        params![id, name, kind, source, order, dwell],
    )
}

pub fn playlist_members(conn: &Connection, id: i64) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn
        .prepare("SELECT key FROM playlist_members WHERE playlist_id=?1 ORDER BY position, key")?;
    let rows = stmt.query_map(params![id], |row| row.get::<_, String>(0))?;
    rows.collect()
}

pub fn playlist_member_items(
    conn: &Connection,
    id: i64,
) -> rusqlite::Result<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(
        "SELECT pm.key, m.name, m.type, m.thumb, m.we_id, m.video_file
         FROM playlist_members pm LEFT JOIN meta m ON m.key = pm.key
         WHERE pm.playlist_id = ?1 ORDER BY pm.position, pm.key",
    )?;
    let rows = stmt.query_map(params![id], |row| {
        Ok(serde_json::json!({
            "key": row.get::<_, String>(0)?,
            "name": row.get::<_, Option<String>>(1)?,
            "type": row.get::<_, Option<String>>(2)?,
            "thumb": row.get::<_, Option<String>>(3)?,
            "we_id": row.get::<_, Option<String>>(4)?,
            "video_file": row.get::<_, Option<String>>(5)?,
        }))
    })?;
    rows.collect()
}

pub fn playlist_add_member(conn: &Connection, id: i64, key: &str) -> rusqlite::Result<()> {
    let pos: i64 = conn.query_row(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_members WHERE playlist_id=?1",
        params![id],
        |row| row.get(0),
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO playlist_members(playlist_id, key, position) VALUES(?1, ?2, ?3)",
        params![id, key, pos],
    )?;
    Ok(())
}

pub fn playlist_remove_member(conn: &Connection, id: i64, key: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM playlist_members WHERE playlist_id=?1 AND key=?2", params![id, key])?;
    Ok(())
}

pub fn playlist_toggle_member(conn: &Connection, id: i64, key: &str) -> rusqlite::Result<bool> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM playlist_members WHERE playlist_id=?1 AND key=?2",
            params![id, key],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if exists {
        playlist_remove_member(conn, id, key)?;
        Ok(false)
    } else {
        playlist_add_member(conn, id, key)?;
        Ok(true)
    }
}

pub fn playlist_move_member(
    conn: &Connection,
    id: i64,
    key: &str,
    delta: i64,
) -> rusqlite::Result<()> {
    let mut keys = playlist_members(conn, id)?;
    let Some(idx) = keys.iter().position(|mkey| mkey == key) else {
        return Ok(());
    };
    let target = idx as i64 + delta;
    if target < 0 || target as usize >= keys.len() {
        return Ok(());
    }
    keys.swap(idx, target as usize);
    for (idx, mkey) in keys.iter().enumerate() {
        conn.execute(
            "UPDATE playlist_members SET position=?3 WHERE playlist_id=?1 AND key=?2",
            params![id, mkey, idx as i64],
        )?;
    }
    Ok(())
}

pub fn playlist_memberships_for_key(conn: &Connection, key: &str) -> rusqlite::Result<Vec<i64>> {
    let mut stmt = conn.prepare("SELECT playlist_id FROM playlist_members WHERE key=?1")?;
    let rows = stmt.query_map(params![key], |row| row.get::<_, i64>(0))?;
    rows.collect()
}

pub fn delete_member_by_key(conn: &Connection, key: &str) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM playlist_members WHERE key=?1", params![key])
}

#[path = "playlists_tests.rs"]
mod tests;
