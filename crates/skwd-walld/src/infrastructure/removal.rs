use std::sync::Arc;
use std::thread;

use serde_json::json;
use skwd_wall_core::{WallState, db};
use wall_proto::{Request, Response, ev};

use crate::backend::events::EventPublisher;
use crate::infrastructure::effects_preview::within_dir;
use crate::infrastructure::events::EventHub;
use crate::infrastructure::stats::Stats;
use crate::infrastructure::watcher::handle_remove;

fn unique_trash_path(trash: &std::path::Path, name: &str) -> std::path::PathBuf {
    let base = trash.join(name);
    if !base.exists() {
        return base;
    }
    for idx in 1.. {
        let cand = trash.join(format!("{name}.{idx}"));
        if !cand.exists() {
            return cand;
        }
    }
    unreachable!()
}

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

pub(crate) fn trash_dir(
    src: &std::path::Path,
    trash: &std::path::Path,
    name: &str,
) -> std::io::Result<()> {
    let dest = unique_trash_path(trash, name);
    if std::fs::rename(src, &dest).is_ok() {
        return Ok(());
    }
    copy_dir_all(src, &dest)?;
    std::fs::remove_dir_all(src)
}

pub(crate) fn trash_file(src: &std::path::Path, trash: &std::path::Path) -> std::io::Result<()> {
    let name = src
        .file_name()
        .map_or_else(|| String::from("file"), |name| name.to_string_lossy().into_owned());
    let dest = unique_trash_path(trash, &name);
    if std::fs::rename(src, &dest).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, &dest)?;
    std::fs::remove_file(src)
}

fn trash_symlink(
    src: &std::path::Path,
    trash: &std::path::Path,
    name: &str,
) -> std::io::Result<()> {
    let dest = unique_trash_path(trash, name);
    if std::fs::rename(src, &dest).is_ok() {
        return Ok(());
    }
    let target = std::fs::read_link(src)?;
    std::os::unix::fs::symlink(target, &dest)?;
    std::fs::remove_file(src)
}

fn canonical_link_target(link: &std::path::Path) -> Option<std::path::PathBuf> {
    let target = std::fs::read_link(link).ok()?;
    let target = if target.is_absolute() { target } else { link.parent()?.join(target) };
    std::fs::canonicalize(target).ok()
}

fn is_lexical_child(path: &std::path::Path, dir: &std::path::Path) -> bool {
    let Ok(relative) = path.strip_prefix(dir) else {
        return false;
    };
    !relative.as_os_str().is_empty()
        && relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

fn reconcile_removed(state: &Arc<WallState>, key: &str, path: &str, we_id: &str) {
    let _ = state.with_db(|conn| db::delete_member_by_key(conn, key));
    if !path.is_empty() {
        state.theme().clear_source_if(path);
        if let Ok(Some(dest)) = state.with_db(|conn| db::tinier_convert_delete(conn, path)) {
            let _ = std::fs::remove_file(&dest);
        }
    }
    crate::infrastructure::persistence::forget_last_wallpaper_if(path, we_id);
    crate::infrastructure::restore_policy::forget_wallpaper(
        &skwd_wall_core::paths::cache_dir().display().to_string(),
        path,
        we_id,
    );
}

pub(crate) fn handle_wall_remove(
    ctx: &crate::composition::context::Ctx,
    req: &Request,
) -> Response {
    let crate::composition::context::Ctx { state, events, stats, .. } = ctx;
    let we_id = req.opt_str("we_id").unwrap_or_default();
    if !we_id.is_empty() {
        return remove_we_item(state, req, events, stats, we_id);
    }
    let path = req.opt_str("path").unwrap_or_default();
    if path.is_empty() {
        return crate::infrastructure::rpc::fail_msg(stats, req.id, 1, "wall.remove needs a path");
    }
    let file = std::path::Path::new(&path);
    let (wdir, vdir) = {
        let cfg = state.config();
        (cfg.wallpaper_dir(), cfg.video_dir())
    };
    let key = skwd_wall_core::paths::key_for_path(file, &wdir, &vdir);
    let missing = matches!(
        std::fs::symlink_metadata(file),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );
    let confined = if missing {
        is_lexical_child(file, std::path::Path::new(&wdir))
            || is_lexical_child(file, std::path::Path::new(&vdir))
    } else {
        within_dir(file, std::path::Path::new(&wdir))
            || within_dir(file, std::path::Path::new(&vdir))
    };
    if key.is_none() || !confined {
        return crate::infrastructure::rpc::fail_msg(
            stats,
            req.id,
            1,
            "wall.remove: path is outside the wallpaper/video dirs",
        );
    }
    if !missing {
        let trash = skwd_wall_core::paths::cache_dir().join("deleted");
        let _ = std::fs::create_dir_all(&trash);
        if trash_file(file, &trash).is_err() {
            let _ = std::fs::remove_file(file);
        }
    }
    reconcile_removed(state, &key.expect("validated above"), path, "");
    handle_remove(state, events.as_ref(), file);
    log::info!("wall.remove: trashed {path}");
    Response::ok(req.id, json!({"removed": path}))
}

fn remove_we_item(
    state: &Arc<WallState>,
    req: &Request,
    events: &Arc<EventHub>,
    stats: &Arc<Stats>,
    we_id: &str,
) -> Response {
    if !skwd_wall_core::paths::safe_component(we_id) {
        return crate::infrastructure::rpc::fail_msg(
            stats,
            req.id,
            1,
            "wall.remove: invalid we_id",
        );
    }
    let (dir, approved_content_target) = {
        let config = state.config();
        let dir = config.we_dir().join(we_id);
        let target = canonical_link_target(&dir);
        let expected =
            config.steam_install_root().join("steamapps/workshop/content/431960").join(we_id);
        let approved = target.filter(|target| {
            std::fs::canonicalize(&expected).is_ok_and(|expected| expected == *target)
        });
        (dir, approved)
    };
    if !dir.is_dir() {
        return crate::infrastructure::rpc::fail_msg(
            stats,
            req.id,
            1,
            "wall.remove: no such WE item",
        );
    }
    let trash = skwd_wall_core::paths::cache_dir().join("deleted");
    let _ = std::fs::create_dir_all(&trash);
    let is_symlink =
        std::fs::symlink_metadata(&dir).is_ok_and(|meta| meta.file_type().is_symlink());
    let result = if is_symlink {
        trash_symlink(&dir, &trash, we_id)
    } else {
        trash_dir(&dir, &trash, we_id)
    };
    if let Err(err) = result {
        return crate::infrastructure::rpc::fail_msg(
            stats,
            req.id,
            1,
            format!("wall.remove: {err}"),
        );
    }
    if let Some(target) = approved_content_target {
        let _ = trash_dir(&target, &trash, &format!("{we_id}.content"));
    }
    let key = format!("we:{we_id}");
    let _ = state.with_db(|conn| db::delete_entries(conn, std::slice::from_ref(&key)));
    let _ = std::fs::remove_file(skwd_wall_core::paths::we_thumb(we_id));
    let _ = std::fs::remove_file(skwd_wall_core::paths::we_thumb_sm(we_id));
    reconcile_removed(state, &key, "", we_id);
    events.publish(ev::REMOVED, json!({ "key": key }));
    crate::infrastructure::semantic_index::request_refresh();
    log::info!("wall.remove: trashed WE item {we_id}");
    if we_id.chars().all(|ch| ch.is_ascii_digit()) {
        let id = we_id.to_string();
        let expected = state.config().steam_backend() != "steamcmd";
        let events = Arc::clone(events);
        thread::spawn(move || {
            crate::infrastructure::steam_download::run_unsubscribe(&id, expected, events.as_ref());
        });
    }
    Response::ok(req.id, json!({ "removed": key }))
}
