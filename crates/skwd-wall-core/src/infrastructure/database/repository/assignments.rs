use rusqlite::{Connection, params};

pub fn playlist_assign_set(
    conn: &Connection,
    output: &str,
    id: Option<i64>,
) -> rusqlite::Result<()> {
    match id {
        Some(pid) => {
            conn.execute(
                "INSERT INTO playlist_assign(output, playlist_id) VALUES(?1, ?2)
                 ON CONFLICT(output) DO UPDATE SET playlist_id=?2",
                params![output, pid],
            )?;
        }
        None => {
            conn.execute("DELETE FROM playlist_assign WHERE output=?1", params![output])?;
        }
    }
    Ok(())
}

pub fn playlist_assigns(conn: &Connection) -> rusqlite::Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare("SELECT output, playlist_id FROM playlist_assign")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
    rows.collect()
}

pub fn playlist_assign_clear(conn: &Connection, id: Option<i64>) -> rusqlite::Result<usize> {
    match id {
        Some(pid) => conn.execute("DELETE FROM playlist_assign WHERE playlist_id=?1", params![pid]),
        None => conn.execute("DELETE FROM playlist_assign", []),
    }
}
