use rusqlite::{Connection, params};

pub fn image_optimization_records(
    connection: &Connection,
) -> rusqlite::Result<Vec<(String, String, u64, i64)>> {
    let mut statement =
        connection.prepare("SELECT src, preset, orig_size, optimized_at FROM image_optimize")?;
    statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn image_optimization_record(
    connection: &Connection,
    source: &str,
    destination: &str,
    profile: &str,
    format: &str,
    width: u32,
    height: u32,
    original_size: u64,
    new_size: u64,
) -> rusqlite::Result<()> {
    let optimized_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .cast_signed();
    connection.execute(
        "INSERT INTO image_optimize(src,dest,preset,format,width,height,orig_size,new_size,optimized_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
         ON CONFLICT(src) DO UPDATE SET
           dest=excluded.dest,preset=excluded.preset,format=excluded.format,
           width=excluded.width,height=excluded.height,orig_size=excluded.orig_size,
           new_size=excluded.new_size,optimized_at=excluded.optimized_at",
        params![
            source,
            destination,
            profile,
            format,
            i64::from(width),
            i64::from(height),
            i64::try_from(original_size).unwrap_or(i64::MAX),
            i64::try_from(new_size).unwrap_or(i64::MAX),
            optimized_at,
        ],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn record_image_optimization_and_rename(
    connection: &Connection,
    source: &str,
    destination: &str,
    profile: &str,
    format: &str,
    width: u32,
    height: u32,
    original_size: u64,
    new_size: u64,
    mtime: i64,
    old_key: &str,
    new_key: &str,
    new_name: &str,
) -> rusqlite::Result<()> {
    let transaction = connection.unchecked_transaction()?;
    let optimized_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .cast_signed();
    transaction.execute(
        "INSERT INTO image_optimize(src,dest,preset,format,width,height,orig_size,new_size,optimized_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
         ON CONFLICT(src) DO UPDATE SET
           dest=excluded.dest,preset=excluded.preset,format=excluded.format,
           width=excluded.width,height=excluded.height,orig_size=excluded.orig_size,
           new_size=excluded.new_size,optimized_at=excluded.optimized_at",
        params![
            source,
            destination,
            profile,
            format,
            i64::from(width),
            i64::from(height),
            i64::try_from(original_size).unwrap_or(i64::MAX),
            i64::try_from(new_size).unwrap_or(i64::MAX),
            optimized_at,
        ],
    )?;
    rename_wallpaper_key_in(&transaction, old_key, new_key, new_name)?;
    transaction.execute(
        "UPDATE meta SET mtime=?1, filesize=?2, width=?3, height=?4 WHERE key=?5",
        params![
            mtime,
            i64::try_from(new_size).unwrap_or(i64::MAX),
            i64::from(width),
            i64::from(height),
            new_key,
        ],
    )?;
    transaction.commit()
}

pub fn rollback_image_optimization_and_rename(
    connection: &Connection,
    source: &str,
    old_key: &str,
    new_key: &str,
    old_name: &str,
) -> rusqlite::Result<()> {
    let transaction = connection.unchecked_transaction()?;
    transaction.execute("DELETE FROM image_optimize WHERE src=?1", [source])?;
    rename_wallpaper_key_in(&transaction, new_key, old_key, old_name)?;
    transaction.commit()
}

pub fn rename_wallpaper_key(
    connection: &Connection,
    old_key: &str,
    new_key: &str,
    new_name: &str,
) -> rusqlite::Result<()> {
    let transaction = connection.unchecked_transaction()?;
    rename_wallpaper_key_in(&transaction, old_key, new_key, new_name)?;
    transaction.commit()
}

fn rename_wallpaper_key_in(
    connection: &Connection,
    old_key: &str,
    new_key: &str,
    new_name: &str,
) -> rusqlite::Result<()> {
    connection.execute(
        "UPDATE meta SET key=?1, name=?2 WHERE key=?3",
        params![new_key, new_name, old_key],
    )?;
    connection
        .execute("UPDATE analysis_tags SET key=?1 WHERE key=?2", params![new_key, old_key])?;
    connection
        .execute("UPDATE analysis_candidates SET key=?1 WHERE key=?2", params![new_key, old_key])?;
    connection
        .execute("UPDATE playlist_members SET key=?1 WHERE key=?2", params![new_key, old_key])?;
    Ok(())
}

#[cfg(test)]
#[path = "maintenance_tests.rs"]
mod tests;
