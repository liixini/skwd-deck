use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

const CATALOG_SOURCES: &str = "SELECT key, COALESCE(name, ''), COALESCE(video_file, '') FROM meta";
const SET_EXCLUDED: &str =
    "UPDATE meta SET library_excluded = ?1 WHERE key = ?2 AND library_excluded != ?1";
const VISIBLE_ENTRY: &str =
    "SELECT NOT EXISTS(SELECT 1 FROM meta WHERE key = ?1 AND library_excluded != 0)";
const LIBRARY_POLICY_KEY: &str = "library.policy";
const WRITE_POLICY: &str =
    "INSERT INTO state(key, val) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET val = excluded.val";
const READ_POLICY: &str = "SELECT val FROM state WHERE key = ?1";
const VIDEO_SOURCE_COLUMN: usize = 2;

pub fn reconcile_library(
    connection: &Connection,
    wallpaper: &Path,
    policy: &skwd_config::LibraryPolicy,
) -> rusqlite::Result<usize> {
    let transaction = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
    let encoded = serde_json::to_string(&(wallpaper, policy))
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    transaction.execute(WRITE_POLICY, params![LIBRARY_POLICY_KEY, encoded])?;
    let mut statement = transaction.prepare(CATALOG_SOURCES)?;
    let entries = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(VIDEO_SOURCE_COLUMN)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    let mut changed = 0;
    for (key, name, video) in entries {
        let excluded = source_excluded(wallpaper, policy, &key, &name, &video);
        changed += transaction.execute(SET_EXCLUDED, params![excluded, key])?;
    }
    transaction.commit()?;
    Ok(changed)
}

pub(super) fn stored_exclusion(
    connection: &Connection,
    key: &str,
    name: &str,
    video: &str,
) -> rusqlite::Result<bool> {
    let encoded: Option<String> =
        connection.query_row(READ_POLICY, [LIBRARY_POLICY_KEY], |row| row.get(0)).optional()?;
    let Some(encoded) = encoded else {
        return Ok(false);
    };
    let (wallpaper, policy): (PathBuf, skwd_config::LibraryPolicy) = serde_json::from_str(&encoded)
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    Ok(source_excluded(&wallpaper, &policy, key, name, video))
}

fn source_excluded(
    wallpaper: &Path,
    policy: &skwd_config::LibraryPolicy,
    key: &str,
    name: &str,
    video: &str,
) -> bool {
    entry_path(wallpaper, key_kind(key), name, video)
        .is_some_and(|path| !policy.contains(&path) || policy.excludes(&path))
}

pub fn key_kind(key: &str) -> &str {
    key.split_once(':').map(|(kind, _)| kind).unwrap_or_default()
}

pub fn entry_path(wallpaper: &Path, kind: &str, name: &str, video: &str) -> Option<PathBuf> {
    match kind {
        wall_proto::kind::STATIC => Some(wallpaper.join(name)),
        wall_proto::kind::VIDEO => Some(PathBuf::from(video)),
        _ => None,
    }
}

pub fn library_visible(connection: &Connection, key: &str) -> rusqlite::Result<bool> {
    connection.query_row(VISIBLE_ENTRY, [key], |row| row.get(0))
}

#[cfg(test)]
#[path = "visibility_tests.rs"]
mod tests;
