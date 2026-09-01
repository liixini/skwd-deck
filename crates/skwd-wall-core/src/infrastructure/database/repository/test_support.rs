#![cfg(test)]

use rusqlite::Connection;

#[allow(clippy::too_many_arguments)]
pub(crate) fn seed(connection: &Connection, key: &str, name: &str, kind: &str) {
    super::wallpapers::upsert_cache_entry(
        connection, key, kind, name, "/t.webp", "/s.webp", "", "", 100, 4, 50, 200, 12345, 1920,
        1080,
    )
    .unwrap();
}
