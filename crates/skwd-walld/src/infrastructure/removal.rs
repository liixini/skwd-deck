use std::sync::Arc;

use serde_json::json;
use skwd_wall_core::{WallState, db};
use wall_proto::{Request, Response, WallRemoveParams, WallRemoveResult, ev};

use crate::backend::events::EventPublisher;
use crate::infrastructure::effects_preview::within_dir;
use crate::infrastructure::events::EventHub;
use crate::infrastructure::stats::Stats;
use crate::infrastructure::watcher::handle_remove;

const UNSUBSCRIBE_INVALID: &str = "wall.remove: unsubscribe needs a numeric we_id";
const STEAM_STATUS_UNKNOWN: &str =
    "Steam subscription status is unknown; open Steam and try Unsubscribe";

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
    crate::composition::runtime::playlist::reload();
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
    let params: WallRemoveParams = match serde_json::from_value(req.params.clone()) {
        Ok(params) => params,
        Err(error) => {
            return crate::infrastructure::rpc::fail_msg(
                stats,
                req.id,
                1,
                format!("wall.remove: {error}"),
            );
        }
    };
    let we_id = params.we_id.as_deref().unwrap_or_default();
    if params.unsubscribe == Some(true)
        && (we_id.is_empty() || !we_id.chars().all(|ch| ch.is_ascii_digit()))
    {
        return crate::infrastructure::rpc::fail_msg(stats, req.id, 1, UNSUBSCRIBE_INVALID);
    }
    if !we_id.is_empty() {
        return remove_we_item(state, req, events, stats, we_id, params.unsubscribe);
    }
    let path = params.path.as_deref().unwrap_or_default();
    if path.is_empty() {
        return crate::infrastructure::rpc::fail_msg(stats, req.id, 1, "wall.remove needs a path");
    }
    let file = std::path::Path::new(&path);
    let config = state.config().clone();
    let roots = config.library_folders();
    let key = state.library_key(file);
    let missing = matches!(
        std::fs::symlink_metadata(file),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );
    let confined = if missing {
        roots.iter().any(|root| is_lexical_child(file, std::path::Path::new(root)))
    } else {
        roots.iter().any(|root| within_dir(file, std::path::Path::new(root)))
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
    Response::ok(req.id, json!(WallRemoveResult { removed: path.to_string(), unsubscribed: false }))
}

fn remove_we_item(
    state: &Arc<WallState>,
    req: &Request,
    events: &Arc<EventHub>,
    stats: &Arc<Stats>,
    we_id: &str,
    unsubscribe: Option<bool>,
) -> Response {
    if !skwd_wall_core::paths::safe_component(we_id) {
        return crate::infrastructure::rpc::fail_msg(
            stats,
            req.id,
            1,
            "wall.remove: invalid we_id",
        );
    }
    let dir = {
        let config = state.config();
        config.we_dir().join(we_id)
    };
    if !dir.is_dir() {
        return crate::infrastructure::rpc::fail_msg(
            stats,
            req.id,
            1,
            "wall.remove: no such WE item",
        );
    }
    let actual = dir.canonicalize().ok();
    if unsubscribe.is_none()
        && crate::infrastructure::steam_subscriptions::hidden(state, we_id, &dir)
    {
        remove_we_record(state, events.as_ref(), we_id, actual.as_deref(), true);
        return Response::ok(
            req.id,
            json!(WallRemoveResult { removed: format!("we:{we_id}"), unsubscribed: false }),
        );
    }
    let account_remove = unsubscribe == Some(true)
        || (unsubscribe.is_none()
            && crate::infrastructure::steam_subscriptions::subscribed(state, we_id, &dir));
    if !account_remove
        && crate::infrastructure::steam_subscriptions::requires_steam_confirmation(
            state, we_id, &dir,
        )
    {
        crate::infrastructure::steam_subscriptions::reconcile(state, events.as_ref());
        if crate::infrastructure::steam_subscriptions::hidden(state, we_id, &dir) {
            return Response::ok(
                req.id,
                json!(WallRemoveResult { removed: format!("we:{we_id}"), unsubscribed: false }),
            );
        }
        return crate::infrastructure::rpc::fail_msg(stats, req.id, 1, STEAM_STATUS_UNKNOWN);
    }
    let subscribed = if account_remove {
        match crate::infrastructure::steam_subscriptions::unsubscribe(
            state,
            events.as_ref(),
            we_id,
            unsubscribe.is_none(),
        ) {
            Ok(()) => true,
            Err(error) => return crate::infrastructure::rpc::fail_msg(stats, req.id, 1, error),
        }
    } else {
        false
    };
    if !subscribed && let Err(error) = trash_we_dir(state, we_id, &dir) {
        return crate::infrastructure::rpc::fail_msg(
            stats,
            req.id,
            1,
            format!("wall.remove: {error}"),
        );
    }
    remove_we_record(state, events.as_ref(), we_id, actual.as_deref(), true);
    Response::ok(
        req.id,
        json!(WallRemoveResult { removed: format!("we:{we_id}"), unsubscribed: subscribed }),
    )
}

fn trash_we_dir(state: &Arc<WallState>, we_id: &str, dir: &std::path::Path) -> std::io::Result<()> {
    let approved_content_target = {
        let config = state.config();
        let target = canonical_link_target(dir);
        let expected = config
            .steam_install_root()
            .join(skwd_wall_core::paths::STEAM_WORKSHOP_CONTENT)
            .join(we_id);
        target.filter(|target| {
            std::fs::canonicalize(&expected).is_ok_and(|expected| expected == *target)
        })
    };
    let trash = skwd_wall_core::paths::cache_dir().join("deleted");
    let _ = std::fs::create_dir_all(&trash);
    let is_symlink = std::fs::symlink_metadata(dir).is_ok_and(|meta| meta.file_type().is_symlink());
    let result =
        if is_symlink { trash_symlink(dir, &trash, we_id) } else { trash_dir(dir, &trash, we_id) };
    result?;
    if let Some(target) = approved_content_target {
        let _ = trash_dir(&target, &trash, &format!("{we_id}.content"));
    }
    Ok(())
}

pub(crate) fn remove_we_record(
    state: &Arc<WallState>,
    events: &dyn EventPublisher,
    we_id: &str,
    actual: Option<&std::path::Path>,
    announce: bool,
) {
    let key = format!("we:{we_id}");
    let deleted =
        state.with_db(|conn| db::delete_entries(conn, std::slice::from_ref(&key))).unwrap_or(0);
    let _ = std::fs::remove_file(skwd_wall_core::paths::we_thumb(we_id));
    let _ = std::fs::remove_file(skwd_wall_core::paths::we_thumb_sm(we_id));
    reconcile_removed(state, &key, "", we_id);
    if let Err(error) = skwd_wall_core::plasma::activities::forget_wallpaper(
        &state.config().we_dir(),
        we_id,
        actual,
    )
    .and_then(|()| skwd_wall_core::plasma::activities::sync_current(state).map(|_| ()))
    {
        log::warn!("wall.remove: Plasma Activity cleanup failed for {we_id}: {error:#}");
    }
    if announce || deleted > 0 {
        events.publish(ev::REMOVED, json!({ "key": key }));
        crate::infrastructure::semantic_index::request_refresh();
        log::info!("wall.remove: removed WE item {we_id}");
    }
}
