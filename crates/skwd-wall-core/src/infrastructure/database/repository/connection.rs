use rusqlite::Connection;

use crate::paths;

pub fn open() -> rusqlite::Result<Connection> {
    let path = paths::db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
        skwd_log::secure_mode(parent, 0o700);
    }
    match open_at(&path) {
        Ok(connection) => Ok(connection),
        Err(error) if is_corruption(&error) => {
            let quarantined = quarantine(&path);
            log::error!(
                "wall.sqlite is corrupt ({error}); moved it to {quarantined} and starting fresh - favourites/playlists/tags from it live in that file"
            );
            open_at(&path)
        }
        Err(error) => Err(error),
    }
}

pub(super) fn open_at(path: &std::path::Path) -> rusqlite::Result<Connection> {
    let connection = Connection::open(path)?;
    connection.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA cache_size=-2000; PRAGMA synchronous=NORMAL;",
    )?;
    migrate(&connection)?;
    secure_database_files(path);
    Ok(connection)
}

fn secure_database_files(path: &std::path::Path) {
    skwd_log::secure_mode(path, 0o600);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_owned();
        sidecar.push(suffix);
        skwd_log::secure_mode(std::path::Path::new(&sidecar), 0o600);
    }
}

pub(super) fn is_corruption(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase)
    )
}

pub(super) fn quarantine(path: &std::path::Path) -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let mut destination = path.as_os_str().to_owned();
    destination.push(format!(".corrupt-{timestamp}"));
    let _ = std::fs::rename(path, &destination);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_owned();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(std::path::Path::new(&sidecar));
    }
    std::path::Path::new(&destination).display().to_string()
}

pub fn open_in_memory() -> rusqlite::Result<Connection> {
    let connection = Connection::open_in_memory()?;
    migrate(&connection)?;
    Ok(connection)
}

pub(super) fn migrate(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta(
            key TEXT PRIMARY KEY,
            tags TEXT,
            tags_raw TEXT,
            colors TEXT,
            matugen TEXT,
            favourite INTEGER DEFAULT 0,
            type TEXT,
            name TEXT,
            thumb TEXT,
            thumb_sm TEXT,
            video_file TEXT,
            we_id TEXT,
            mtime INTEGER,
            hue INTEGER DEFAULT 99,
            sat INTEGER DEFAULT 0,
            richness INTEGER DEFAULT 0,
            analyzed_by TEXT,
            analysis_error TEXT,
            filesize INTEGER,
            width INTEGER,
            height INTEGER,
            duration_ms INTEGER,
            weather TEXT,
            apply_count INTEGER DEFAULT 0,
            last_applied INTEGER DEFAULT 0,
            generated_tags TEXT,
            user_tags TEXT,
            manual_locked INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_meta_favourite ON meta(favourite);
        CREATE INDEX IF NOT EXISTS idx_meta_type ON meta(type);
        CREATE INDEX IF NOT EXISTS idx_meta_name ON meta(name);
        CREATE INDEX IF NOT EXISTS idx_meta_we_id ON meta(we_id);

        CREATE TABLE IF NOT EXISTS image_optimize(
            src TEXT PRIMARY KEY,
            dest TEXT NOT NULL,
            preset TEXT NOT NULL,
            format TEXT,
            width INTEGER,
            height INTEGER,
            orig_size INTEGER,
            new_size INTEGER,
            optimized_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS video_convert(
            src TEXT PRIMARY KEY,
            dest TEXT NOT NULL,
            preset TEXT NOT NULL,
            codec TEXT,
            width INTEGER,
            height INTEGER,
            orig_size INTEGER,
            new_size INTEGER,
            converted_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS tinier_convert(
            src TEXT PRIMARY KEY,
            dest TEXT NOT NULL,
            frame_rate TEXT NOT NULL,
            preset TEXT NOT NULL,
            orig_size INTEGER,
            new_size INTEGER,
            converted_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS effect_tags(
            stem TEXT PRIMARY KEY,
            tag TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS analysis_tags(
            key TEXT NOT NULL,
            tag TEXT NOT NULL,
            score REAL NOT NULL,
            source TEXT NOT NULL,
            PRIMARY KEY(key, tag, source)
        ) WITHOUT ROWID;
        CREATE INDEX IF NOT EXISTS idx_analysis_tags_tag ON analysis_tags(tag);
        CREATE TABLE IF NOT EXISTS analysis_candidates(
            key TEXT NOT NULL,
            tag TEXT NOT NULL,
            score REAL NOT NULL,
            source TEXT NOT NULL,
            PRIMARY KEY(key, tag, source)
        ) WITHOUT ROWID;
        CREATE INDEX IF NOT EXISTS idx_analysis_candidates_key ON analysis_candidates(key);
        CREATE TRIGGER IF NOT EXISTS meta_delete_analysis_tags
        AFTER DELETE ON meta
        BEGIN
            DELETE FROM analysis_tags WHERE key = OLD.key;
        END;
        CREATE TRIGGER IF NOT EXISTS meta_delete_analysis_candidates
        AFTER DELETE ON meta
        BEGIN
            DELETE FROM analysis_candidates WHERE key = OLD.key;
        END;

        CREATE TABLE IF NOT EXISTS we_properties(
            we_id TEXT NOT NULL,
            name TEXT NOT NULL,
            value TEXT NOT NULL,
            PRIMARY KEY(we_id, name)
        ) WITHOUT ROWID;

        CREATE TABLE IF NOT EXISTS state(
            key TEXT PRIMARY KEY,
            val TEXT
        );

        CREATE TABLE IF NOT EXISTS playlists(
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            kind TEXT NOT NULL DEFAULT 'curated',
            source TEXT,
            order_mode TEXT NOT NULL DEFAULT 'shuffle',
            dwell INTEGER NOT NULL DEFAULT 600,
            position INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS playlist_members(
            playlist_id INTEGER NOT NULL,
            key TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY(playlist_id, key)
        );
        CREATE INDEX IF NOT EXISTS idx_plmembers_pl ON playlist_members(playlist_id);

        CREATE TABLE IF NOT EXISTS playlist_assign(
            output TEXT PRIMARY KEY,
            playlist_id INTEGER NOT NULL
        );",
    )?;
    ensure_meta_columns(connection)?;
    let version =
        connection.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?;
    if version < 4 {
        connection.execute("UPDATE meta SET mtime = 0 WHERE type = 'video'", [])?;
    }
    purge_retired_generated_wallpapers(connection)?;
    migrate_tag_ownership(connection)?;
    connection.pragma_update(None, "user_version", 4)?;
    Ok(())
}

pub(super) const META_COLUMNS: &[(&str, &str)] = &[
    ("tags", "TEXT"),
    ("tags_raw", "TEXT"),
    ("colors", "TEXT"),
    ("matugen", "TEXT"),
    ("favourite", "INTEGER DEFAULT 0"),
    ("type", "TEXT"),
    ("name", "TEXT"),
    ("thumb", "TEXT"),
    ("thumb_sm", "TEXT"),
    ("video_file", "TEXT"),
    ("we_id", "TEXT"),
    ("mtime", "INTEGER"),
    ("hue", "INTEGER DEFAULT 99"),
    ("sat", "INTEGER DEFAULT 0"),
    ("richness", "INTEGER DEFAULT 0"),
    ("analyzed_by", "TEXT"),
    ("analysis_error", "TEXT"),
    ("filesize", "INTEGER"),
    ("width", "INTEGER"),
    ("height", "INTEGER"),
    ("duration_ms", "INTEGER"),
    ("weather", "TEXT"),
    ("apply_count", "INTEGER DEFAULT 0"),
    ("last_applied", "INTEGER DEFAULT 0"),
    ("generated_tags", "TEXT"),
    ("user_tags", "TEXT"),
    ("manual_locked", "INTEGER NOT NULL DEFAULT 0"),
];

fn ensure_meta_columns(connection: &Connection) -> rusqlite::Result<()> {
    let existing: std::collections::HashSet<String> = {
        let mut statement = connection.prepare("PRAGMA table_info(meta)")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
        rows.filter_map(std::result::Result::ok).collect()
    };
    for (name, declaration) in META_COLUMNS {
        if !existing.contains(*name) {
            connection
                .execute_batch(&format!("ALTER TABLE meta ADD COLUMN {name} {declaration};"))?;
        }
    }
    Ok(())
}

// Runs on every open: a stale pre-revert daemon can still write gen/shader rows.
fn purge_retired_generated_wallpapers(connection: &Connection) -> rusqlite::Result<()> {
    let transaction = connection.unchecked_transaction()?;
    transaction.execute(
        "DELETE FROM playlist_members
         WHERE key LIKE 'gen:%' OR key LIKE 'shader:%'",
        [],
    )?;
    transaction.execute(
        "DELETE FROM meta
         WHERE type IN ('gen', 'shader') OR key LIKE 'gen:%' OR key LIKE 'shader:%'",
        [],
    )?;
    transaction.commit()
}

fn migrate_tag_ownership(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "UPDATE meta
         SET generated_tags = tags
         WHERE generated_tags IS NULL
           AND user_tags IS NULL
           AND COALESCE(tags, '') != ''
           AND EXISTS(SELECT 1 FROM analysis_tags WHERE analysis_tags.key = meta.key);

         UPDATE meta
         SET user_tags = tags, manual_locked = 1
         WHERE generated_tags IS NULL
           AND user_tags IS NULL
           AND COALESCE(tags, '') != ''
           AND NOT EXISTS(SELECT 1 FROM analysis_tags WHERE analysis_tags.key = meta.key);",
    )
}
