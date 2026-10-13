use std::sync::Arc;
use std::thread;

use serde_json::json;
use skwd_wall_core::WallState;
use skwd_wall_core::backend::wallpaper::{
    ApplyOutputRequest, ApplyVideoRequest, StaticSmartRequest, WallpaperApplication,
};
use wall_proto::ev;

use crate::backend::events::EventPublisher;
use crate::backend::history::ApplySource;
use crate::composition::apply::{GenerationRequest, SupersededApply, apply_core_locked};
use crate::composition::context::Ctx;

const RESTORE_RETRY_DELAYS: [std::time::Duration; 4] = [
    std::time::Duration::from_secs(1),
    std::time::Duration::from_secs(2),
    std::time::Duration::from_secs(4),
    std::time::Duration::from_secs(8),
];
use crate::domain::hotplug::{
    AppliedKind, HotplugPlan, hotplug_plan, newly_appeared_outputs, per_output_state_is_divergent,
    representative as choose_representative, retained_outputs,
};
use crate::infrastructure::persistence::last_wallpaper_path;
use skwd_wall_core::domain::wallpaper as restore;

fn persisted_outputs(map: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    map.keys().cloned().collect()
}

fn applied_kind(entry: &serde_json::Value) -> AppliedKind {
    match entry.get("type").and_then(serde_json::Value::as_str).unwrap_or(wall_proto::kind::STATIC)
    {
        wall_proto::kind::VIDEO => AppliedKind::Video,
        wall_proto::kind::STATIC => AppliedKind::Static,
        _ => AppliedKind::Other,
    }
}

fn entry_fields(entry: &serde_json::Value) -> (String, String, String, bool, u32) {
    let ty = entry
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(wall_proto::kind::STATIC)
        .to_string();
    let path = entry.get("path").and_then(serde_json::Value::as_str).unwrap_or("").to_string();
    let we_id = entry.get("we_id").and_then(serde_json::Value::as_str).unwrap_or("").to_string();
    let mute = entry.get("mute").and_then(serde_json::Value::as_bool).unwrap_or(true);
    let volume = entry.get("volume").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
    (ty, path, we_id, mute, volume)
}

fn last_wallpaper_entry_fields() -> Option<(String, String, String, bool, u32)> {
    let text = std::fs::read_to_string(last_wallpaper_path()).ok()?;
    let val: serde_json::Value = serde_json::from_str(&text).ok()?;
    let ty = val.get("type").and_then(serde_json::Value::as_str)?;
    let path = val.get("path").and_then(serde_json::Value::as_str).unwrap_or("");
    let we_id = val.get("we_id").and_then(serde_json::Value::as_str).unwrap_or("");
    if (ty == wall_proto::kind::WE && we_id.is_empty())
        || (ty != wall_proto::kind::WE && path.is_empty())
    {
        return None;
    }
    let mute = val.get("mute").and_then(serde_json::Value::as_bool).unwrap_or(true);
    let volume = val.get("volume").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
    Some((ty.to_string(), path.to_string(), we_id.to_string(), mute, volume))
}

fn hotplug_representative(
    map: &serde_json::Map<String, serde_json::Value>,
    live: &[String],
) -> Option<(String, serde_json::Value)> {
    let selection = choose_representative(&persisted_outputs(map), live)?;
    map.get(&selection.state_key).cloned().map(|entry| (selection.output, entry))
}

fn reapply_outputs_state(
    state: &Arc<WallState>,
    application: &dyn WallpaperApplication,
    publisher: &dyn EventPublisher,
    source: ApplySource,
) -> anyhow::Result<()> {
    let _apply = state.apply().lock();
    if source == ApplySource::Restore && state.apply().restore_cancelled(None) {
        return Err(SupersededApply.into());
    }
    let activity = skwd_wall_core::plasma::activities::sync_current(state)?;
    if source == ApplySource::Restore
        && state.apply().restore_cancelled(
            activity.as_ref().map(skwd_wall_core::plasma::activities::Target::id),
        )
    {
        return Err(SupersededApply.into());
    }
    let cache = state.config().cache_dir();
    let map = match skwd_wall_core::audio::read_state(&cache).as_object() {
        Some(map) if !map.is_empty() => map.clone(),
        _ if source == ApplySource::Restore => {
            anyhow::bail!("saved Plasma wallpapers are not available yet");
        }
        _ => return Ok(()),
    };
    let live = skwd_wall_core::outputs::names();
    if live.is_empty() {
        anyhow::bail!("restore outputs are unavailable");
    }
    let persisted = persisted_outputs(&map);
    let plan = hotplug_plan(
        &persisted,
        map.get("*").map(applied_kind),
        application.video_engine_is_vk(),
        state.renderers().has_video_paper("*"),
        state.renderers().has_base_still(),
    );
    if plan != HotplugPlan::Reconcile {
        let entry = map.get("*").cloned().unwrap_or_default();
        let (ty, path, _, mute, volume) = entry_fields(&entry);
        let fill_mode = state.config().display().fill_mode();
        match plan {
            HotplugPlan::KeepAlive => {
                log::info!("hotplug: uniform '*' {ty} renderer manages its own surfaces, keeping");
                Ok(())
            }
            HotplugPlan::RespawnUniformVideo => {
                log::info!("hotplug: respawning uniform '*' video for the new output set");
                application.apply_video(ApplyVideoRequest {
                    output: "*",
                    path: &path,
                    fill_mode: &fill_mode,
                    mute,
                    volume,
                })
            }
            HotplugPlan::RespawnUniformStatic => {
                log::info!("hotplug: respawning uniform '*' still");
                application.apply_static_smart(StaticSmartRequest {
                    output: "*",
                    path: &path,
                    fill_mode: &fill_mode,
                })
            }
            HotplugPlan::Reconcile => unreachable!(),
        }?;
        publisher.publish(ev::OUTPUTS_CHANGED, json!({ "outputs": live }));
        return Ok(());
    }
    prune_departed_outputs(&cache, &live);
    let pruned = skwd_wall_core::audio::read_state(&cache).as_object().cloned().unwrap_or_default();
    seed_new_outputs(&cache, &pruned, &live);
    let map = skwd_wall_core::audio::read_state(&cache).as_object().cloned().unwrap_or_default();
    let Some((rep_out, entry)) = hotplug_representative(&map, &live) else {
        return Ok(());
    };
    let (ty, path, we_id, mute, volume) = entry_fields(&entry);
    let fill_mode = state.config().display().fill_mode_for(&rep_out);
    log::info!(
        "hotplug: reconciling {} live output(s) from outputs.json (rep={rep_out})",
        live.len()
    );
    match application.apply_output(ApplyOutputRequest {
        output: &rep_out,
        kind: &ty,
        path: &path,
        we_id: &we_id,
        fill_mode: &fill_mode,
        mute,
        volume,
        frame_rate: None,
        transition: None,
    }) {
        Ok(()) => {
            publisher.publish(ev::OUTPUTS_CHANGED, json!({ "outputs": live }));
            Ok(())
        }
        Err(err) => Err(err),
    }
}

fn prune_departed(
    map: &serde_json::Map<String, serde_json::Value>,
    live: &[String],
) -> serde_json::Map<String, serde_json::Value> {
    retained_outputs(&persisted_outputs(map), live)
        .into_iter()
        .filter_map(|key| map.get(&key).cloned().map(|value| (key, value)))
        .collect()
}

fn prune_departed_outputs(cache: &str, live: &[String]) {
    let Some(map) = skwd_wall_core::audio::read_state(cache).as_object().cloned() else {
        return;
    };
    let kept = prune_departed(&map, live);
    if kept.len() != map.len() {
        skwd_wall_core::audio::write_state(cache, &serde_json::Value::Object(kept));
    }
}

fn seed_new_outputs(
    cache: &str,
    map: &serde_json::Map<String, serde_json::Value>,
    live: &[String],
) {
    let persisted = persisted_outputs(map);
    if !per_output_state_is_divergent(&persisted) {
        return;
    }
    let new_outs = newly_appeared_outputs(&persisted, live);
    if new_outs.is_empty() {
        return;
    }
    let Some((ty, path, we_id, mute, volume)) =
        last_wallpaper_entry_fields().or_else(|| map.values().next().map(entry_fields))
    else {
        return;
    };
    for out in &new_outs {
        skwd_wall_core::audio::set_entry(cache, out, &ty, &path, &we_id, mute, volume);
    }
}

pub(crate) fn restore_last(ctx: &Ctx, source: ApplySource) {
    let delays = if source == ApplySource::Restore { RESTORE_RETRY_DELAYS.as_slice() } else { &[] };
    let _ = retry_restore(delays, || {
        restore_pass(ctx, source).inspect_err(|error| log::warn!("restore failed: {error:#}"))
    });
}

fn restore_pass(ctx: &Ctx, source: ApplySource) -> anyhow::Result<()> {
    let state = &ctx.state;
    let application = ctx.wallpaper.as_ref();
    if skwd_wall_core::plasma::available() {
        return reapply_outputs_state(state, application, ctx.events.as_ref(), source)
            .inspect_err(|error| log::warn!("Plasma Activity restore failed: {error:#}"));
    }
    let _planning = state.apply().lock();
    if source == ApplySource::Restore && state.apply().restore_cancelled(None) {
        return Err(SupersededApply.into());
    }
    let outputs = skwd_wall_core::outputs::enumerate();
    let live: Vec<String> = outputs.iter().map(|output| output.name.clone()).collect();
    if outputs.is_empty() {
        anyhow::bail!("restore outputs are unavailable");
    }
    let cache = state.config().cache_dir();
    prune_departed_outputs(&cache, &live);
    let pruned = skwd_wall_core::audio::read_state(&cache).as_object().cloned().unwrap_or_default();
    seed_new_outputs(&cache, &pruned, &live);
    let reaped = state.renderers().kill_departed_outputs(&live);
    if !reaped.is_empty() {
        log::info!("hotplug: reaped renderers for departed outputs: {}", reaped.join(", "));
    }
    let multi_invalidated = state.renderers().retain_live_output_assignments(&live);
    if multi_invalidated {
        state.renderers().kill_video_paper("multi");
    }
    let assignments = state.renderers().assignments();
    if application.video_engine_is_vk()
        && state.renderers().has_video_paper("*")
        && live.iter().any(|output| !assignments.contains_key(output))
    {
        state.renderers().kill_video_paper("*");
    }
    crate::infrastructure::restore_policy::purge_missing(&cache);
    crate::infrastructure::restore_policy::remember_monitors(&outputs);
    let targets = crate::infrastructure::restore_policy::targets(state, &outputs);
    let last = crate::infrastructure::restore_policy::read_last_applied();
    let mut resolved = restore::resolve(&targets, &last);
    if resolved.is_empty()
        && let Some((kind, path, we_id, _, _)) = last_wallpaper_entry_fields()
    {
        let wallpaper = restore::Wallpaper { kind, path, we_id };
        resolved = live
            .iter()
            .map(|output| restore::Resolution {
                output: output.clone(),
                wallpaper: wallpaper.clone(),
                already_applied: false,
            })
            .collect();
    }
    let pending: Vec<_> =
        resolved.into_iter().filter(|resolution| !resolution.already_applied).collect();
    let groups = restore::pending(&pending, &live);
    if groups.is_empty() {
        return Ok(());
    }
    let mut failure = None;
    for group in groups {
        if !crate::infrastructure::restore_policy::wallpaper_present(&group.wallpaper) {
            log::warn!("restore: skipping {}, its wallpaper is gone", group.target);
            continue;
        }
        for target in expand_group_target(&group.target, &live, state) {
            match restore_group(ctx, source, &target, &group.wallpaper) {
                Ok(()) => {}
                Err(error) if error.is::<SupersededApply>() => return Err(error),
                Err(error) => failure = Some(error),
            }
        }
    }
    failure.map_or(Ok(()), Err)
}

fn expand_group_target(target: &str, live: &[String], state: &WallState) -> Vec<String> {
    if target == "*" {
        let config = state.config();
        let recorded = skwd_wall_core::audio::read_state(&config.cache_dir());
        let audio = skwd_wall_core::audio::carried_audio(
            &recorded,
            target,
            config.renderer().mute(),
            config.renderer().volume(),
        );
        if live.iter().any(|output| {
            skwd_wall_core::audio::carried_audio(
                &recorded,
                output,
                config.renderer().mute(),
                config.renderer().volume(),
            ) != audio
        }) {
            return live.to_vec();
        }
        return vec![String::from("*")];
    }
    target.split(',').filter(|name| !name.is_empty()).map(String::from).collect()
}

fn restore_group(
    ctx: &Ctx,
    source: ApplySource,
    output: &str,
    wallpaper: &restore::Wallpaper,
) -> anyhow::Result<()> {
    let state = &ctx.state;
    let (def_mute, def_vol, cache) = {
        let cfg = state.config();
        (cfg.renderer().mute(), cfg.renderer().volume(), cfg.cache_dir())
    };
    let (mute, volume) = skwd_wall_core::audio::resolve_defaults(&cache, output, def_mute, def_vol);
    log::info!(
        "restore: {output} <- {} {}",
        wallpaper.kind,
        if wallpaper.kind == wall_proto::kind::WE { &wallpaper.we_id } else { &wallpaper.path }
    );
    apply_core_locked(
        state,
        ctx.wallpaper.as_ref(),
        ctx.history.as_ref(),
        ctx.events.as_ref(),
        &ctx.stats,
        &wallpaper.kind,
        &wallpaper.path,
        &wallpaper.we_id,
        mute,
        volume,
        source,
        output,
        true,
        false,
        None,
        GenerationRequest::Claim(None),
        None,
    )
    .map(|_| ())
}

fn retry_restore(
    delays: &[std::time::Duration],
    mut apply: impl FnMut() -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let mut delays = delays.iter();
    loop {
        let result = apply();
        if result.is_ok() || result.as_ref().is_err_and(anyhow::Error::is::<SupersededApply>) {
            return result;
        }
        let Some(delay) = delays.next() else { return result };
        thread::sleep(*delay);
    }
}

pub(crate) fn start_output_watch(ctx: crate::composition::context::Ctx) {
    thread::spawn(move || {
        skwd_wall_core::outputs::watch(|| {
            log::info!("output set changed (hotplug), reconciling per-output wallpapers");
            skwd_wall_core::outputs::invalidate();
            crate::infrastructure::reap::kill_stale_renderers(&{
                let cfg = ctx.state.config();
                [cfg.cache_dir() + "/video-opt", cfg.video_dir()]
            });
            restore_last(&ctx, ApplySource::Hotplug);
            ctx.events.publish(
                ev::OUTPUTS_CHANGED,
                json!({ "outputs": skwd_wall_core::outputs::names() }),
            );
            crate::composition::runtime::schedule::reload();
        });
    });
}

#[cfg(test)]
#[path = "hotplug/tests.rs"]
mod tests;
