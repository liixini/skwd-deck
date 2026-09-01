use rusqlite::{Connection, params};
use serde_json::{Map, Value};

pub const MAX_WE_PROPERTIES: usize = 512;
pub const MAX_WE_PROPERTY_NAME: usize = 128;

pub fn we_properties(conn: &Connection, we_id: &str) -> Map<String, Value> {
    let mut out = Map::new();
    let Ok(mut stmt) =
        conn.prepare("SELECT name, value FROM we_properties WHERE we_id=?1 ORDER BY name")
    else {
        return out;
    };
    let Ok(rows) = stmt
        .query_map(params![we_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
    else {
        return out;
    };
    for (name, raw) in rows.flatten() {
        let value = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
        out.insert(name, value);
    }
    out
}

pub fn set_we_property(
    conn: &Connection,
    we_id: &str,
    name: &str,
    value: Option<&Value>,
) -> rusqlite::Result<()> {
    let Some(value) = value else {
        conn.execute("DELETE FROM we_properties WHERE we_id=?1 AND name=?2", params![we_id, name])?;
        return Ok(());
    };
    let encoded = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
    conn.execute(
        "INSERT INTO we_properties(we_id, name, value) VALUES(?1, ?2, ?3)
         ON CONFLICT(we_id, name) DO UPDATE SET value=excluded.value",
        params![we_id, name, encoded],
    )?;
    Ok(())
}

pub fn clear_we_properties(conn: &Connection, we_id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM we_properties WHERE we_id=?1", params![we_id])?;
    Ok(())
}

pub fn valid_property_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= MAX_WE_PROPERTY_NAME
}

#[cfg(test)]
#[path = "we_properties_tests.rs"]
mod tests;
