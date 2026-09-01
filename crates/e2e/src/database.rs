use std::path::Path;

pub fn db_count(sqlite: &Path) -> i64 {
    if !sqlite.exists() {
        return -1;
    }
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY;
    let Ok(conn) = rusqlite::Connection::open_with_flags(sqlite, flags) else {
        return -1;
    };
    conn.query_row(
        "SELECT COUNT(*) FROM meta WHERE key LIKE 'static:%' OR key LIKE 'video:%'",
        [],
        |row| row.get::<_, i64>(0),
    )
    .unwrap_or(-1)
}
