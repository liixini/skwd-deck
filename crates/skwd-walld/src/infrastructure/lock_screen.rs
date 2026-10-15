use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use skwd_config::keys::{lock_screens as keys, plasma as plasma_keys};

use skwd_wall_core::WallState;
use skwd_wall_core::infrastructure::plasma::LockScreenCurrent;
use tokio::sync::{mpsc::UnboundedSender, oneshot};
use wall_proto::{LockScreenSetRequest, LockScreenTarget};

mod export;
mod worker;

pub(crate) use worker::{RENDER_WORKER_ARG, run_render_worker};

#[cfg(test)]
#[path = "lock_screen/tests.rs"]
mod tests;

const CONFIG_MODE: u32 = 0o600;

struct Rollback<'a> {
    state: &'a WallState,
    original: &'a Value,
    config: &'a skwd_wall_core::config::Config,
    current: Option<&'a LockScreenCurrent<'a>>,
    previous: Option<&'a std::path::Path>,
    root: &'a std::path::Path,
    target: LockScreenTarget,
    mode_changed: bool,
    sddm: Option<&'a export::SddmSnapshot>,
    image_attempted: bool,
}

enum Job {
    Sync(Arc<WallState>),
    Set(Arc<WallState>, LockScreenSetRequest, oneshot::Sender<Result<()>>),
}

fn sender() -> &'static UnboundedSender<Job> {
    static SENDER: OnceLock<UnboundedSender<Job>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut pending = None;
            loop {
                let job = match pending.take() {
                    Some(job) => job,
                    None => match receiver.recv().await {
                        Some(job) => job,
                        None => break,
                    },
                };
                let job = if let Job::Sync(mut state) = job {
                    while let Ok(next) = receiver.try_recv() {
                        match next {
                            Job::Sync(latest) => state = latest,
                            set @ Job::Set(..) => {
                                pending = Some(set);
                                break;
                            }
                        }
                    }
                    Job::Sync(state)
                } else {
                    job
                };
                let _ = tokio::task::spawn_blocking(move || {
                    match job {
                        Job::Sync(state) => sync_current(&state),
                        Job::Set(state, request, reply) => {
                            let _ = reply.send(set(&state, &request));
                        }
                    }
                    super::platform::trim_allocator();
                })
                .await;
            }
        });
        sender
    })
}

// The persisted record is global, so on mixed-output setups the last apply wins.
fn sync_current(state: &WallState) {
    let entry = super::persistence::current_entry();
    let poster = entry
        .as_ref()
        .and_then(|entry| {
            super::persistence::last_any_thumb().filter(|poster| {
                entry.ty == wall_proto::kind::STATIC || poster.as_str() != entry.path
            })
        })
        .unwrap_or_default();
    let current = entry.as_ref().map(|entry| LockScreenCurrent {
        kind: &entry.ty,
        path: &entry.path,
        we_id: &entry.we_id,
        poster: &poster,
    });
    sync_images(state, current.as_ref());
    match skwd_wall_core::infrastructure::plasma::sync_lock_screen(state, current.as_ref()) {
        Ok(true) => log::info!("synchronized KDE Plasma lock-screen wallpaper"),
        Ok(false) => {}
        Err(error) => log::warn!("could not synchronize KDE Plasma lock-screen wallpaper: {error}"),
    }
}

fn sync_images(state: &WallState, current: Option<&LockScreenCurrent<'_>>) {
    use skwd_wall_core::infrastructure::lock_screens;

    let config = state.config();
    let root = skwd_wall_core::paths::data_dir().join(lock_screens::DIRECTORY);
    let previous = export::previous(&root).unwrap_or_else(|error| {
        log::warn!("lockscreen: saved image unavailable: {error}");
        None
    });
    let image = if lock_screens::requested(config.root()) {
        match export::render(config.root(), current, &root) {
            Ok(image) => Some(image),
            Err(error) => {
                log::warn!("lockscreen: image export failed: {error}");
                previous.clone()
            }
        }
    } else {
        None
    };
    if let Some(image) = image.as_deref()
        && let Err(error) = export::export_sddm(config.root(), image, &root)
    {
        log::warn!("lockscreen: SDDM export failed: {error}");
    }
    if let Err(error) = lock_screens::sync(config.root(), image.as_deref(), &root) {
        log::warn!("lockscreen: synchronization failed: {error}");
    }
    if let Some(image) = image
        && let Err(error) = export::prune(&root, &image, previous.as_deref())
    {
        log::warn!("lockscreen: old image cleanup failed: {error}");
    }
}

fn put(root: &mut Value, key: &str, value: Value) -> Result<()> {
    let (section, name) = key.rsplit_once('.').context("Invalid lockscreen setting key")?;
    let mut parent = root;
    for part in section.split('.') {
        parent = parent
            .as_object_mut()
            .context("Lockscreen configuration section must be an object")?
            .entry(part)
            .or_insert_with(|| json!({}));
    }
    parent
        .as_object_mut()
        .context("Lockscreen configuration section must be an object")?
        .insert(name.to_owned(), value);
    Ok(())
}

fn candidate(root: &mut Value, request: &LockScreenSetRequest) -> Result<()> {
    let key = match request.target {
        LockScreenTarget::Plasma => plasma_keys::LOCK_SCREEN_MODE,
        LockScreenTarget::Hyprlock => keys::HYPRLOCK,
        LockScreenTarget::Swaylock => keys::SWAYLOCK,
        LockScreenTarget::Dms => keys::DMS,
        LockScreenTarget::Noctalia => keys::NOCTALIA,
        LockScreenTarget::Sddm => keys::SDDM_IMAGE,
    };
    if request.target == LockScreenTarget::Plasma {
        ensure!(request.sddm_image.is_none(), "SDDM image is only valid for SDDM");
        let current = skwd_config::str_ref(root, key, keys::OFF);
        let mode = if !request.enabled {
            keys::OFF
        } else if matches!(current, keys::FOLLOW | keys::STATIC) {
            current
        } else {
            keys::FOLLOW
        };
        put(root, key, json!(mode))?;
    } else {
        if request.enabled {
            let current = skwd_config::str_ref(root, keys::MODE, keys::OFF);
            if !matches!(current, keys::FOLLOW | keys::STATIC) {
                for key in keys::TARGET_FLAGS {
                    put(root, key, json!(false))?;
                }
                put(root, keys::SDDM_IMAGE, json!(""))?;
                put(root, keys::MODE, json!(keys::FOLLOW))?;
            }
        }
        if request.target == LockScreenTarget::Sddm {
            let destination = if request.enabled {
                let image =
                    request.sddm_image.as_deref().context("Missing SDDM image destination")?;
                let expected =
                    skwd_wall_core::infrastructure::lock_screens::sddm::image_for_user(unsafe {
                        libc::geteuid()
                    });
                ensure!(std::path::Path::new(image) == expected, "Invalid SDDM image destination");
                image
            } else {
                ensure!(request.sddm_image.is_none(), "SDDM image is only valid when enabling");
                ""
            };
            put(root, key, json!(destination))?;
        } else {
            ensure!(request.sddm_image.is_none(), "SDDM image is only valid for SDDM");
            put(root, key, json!(request.enabled))?;
        }
    }
    Ok(())
}

fn config_bytes(path: &std::path::Path) -> Result<Vec<u8>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error).context("Read lockscreen configuration"),
    }
}

#[expect(clippy::too_many_lines, reason = "lockscreen setup owns the configuration transaction")]
fn set(state: &WallState, request: &LockScreenSetRequest) -> Result<()> {
    use skwd_wall_core::infrastructure::lock_screens;

    if request.enabled {
        ensure!(
            lock_screens::target_available(request.target),
            "Lockscreen target is not installed"
        );
    }
    let path = skwd_config::config_path();
    lock_screens::check_path(&path)?;
    if let Ok(metadata) = std::fs::metadata(&path) {
        ensure!(
            metadata.is_file() && !metadata.permissions().readonly(),
            "Lockscreen configuration is read-only"
        );
    }
    let original = config_bytes(&path)?;
    let root: Value =
        if original.is_empty() { json!({}) } else { serde_json::from_slice(&original)? };
    let mut next = root.clone();
    candidate(&mut next, request)?;
    let entry = super::persistence::current_entry();
    let poster = entry
        .as_ref()
        .and_then(|entry| {
            super::persistence::last_any_thumb().filter(|poster| {
                entry.ty == wall_proto::kind::STATIC || poster.as_str() != entry.path
            })
        })
        .unwrap_or_default();
    let current = entry.as_ref().map(|entry| LockScreenCurrent {
        kind: &entry.ty,
        path: &entry.path,
        we_id: &entry.we_id,
        poster: &poster,
    });
    let original_config = skwd_wall_core::config::Config::from_root(root.clone());
    let next_config = skwd_wall_core::config::Config::from_root(next.clone());
    let root_dir = skwd_wall_core::paths::data_dir().join(lock_screens::DIRECTORY);
    let previous = export::previous(&root_dir)?;
    let mode_changed = skwd_config::str_ref(&root, keys::MODE, keys::OFF)
        != skwd_config::str_ref(&next, keys::MODE, keys::OFF);
    let sddm = match (request.target, request.enabled) {
        (LockScreenTarget::Sddm, true) => Some(export::sddm_snapshot(&next, &root_dir)?),
        (LockScreenTarget::Sddm, false)
            if !skwd_config::str_ref(&root, keys::SDDM_IMAGE, "").trim().is_empty() =>
        {
            Some(export::sddm_snapshot(&root, &root_dir)?)
        }
        _ => None,
    };
    let rollback = Rollback {
        state,
        original: &root,
        config: &original_config,
        current: current.as_ref(),
        previous: previous.as_deref(),
        root: &root_dir,
        target: request.target,
        mode_changed,
        sddm: sddm.as_ref(),
        image_attempted: request.enabled && request.target != LockScreenTarget::Plasma,
    };
    let result = (|| -> Result<()> {
        if request.target == LockScreenTarget::Plasma {
            let applied = skwd_wall_core::infrastructure::plasma::sync_lock_screen_config(
                state,
                &next_config,
                current.as_ref(),
            )?;
            ensure!(!request.enabled || applied, "No applied wallpaper for Plasma lockscreen");
            Ok(())
        } else {
            let image = if request.enabled {
                Some(export::render(&next, current.as_ref(), &root_dir)?)
            } else {
                None
            };
            if request.target == LockScreenTarget::Sddm {
                if let Some(image) = image.as_deref() {
                    export::export_sddm(&next, image, &root_dir)?;
                } else {
                    export::remove_sddm(&root, &root_dir)?;
                }
                Ok(())
            } else if mode_changed {
                lock_screens::sync(&next, image.as_deref(), &root_dir)
            } else {
                lock_screens::sync_target(&next, image.as_deref(), &root_dir, request.target)
            }
        }
    })();
    if let Err(error) = result {
        rollback.restore().with_context(|| format!("Setup failed: {error:#}"))?;
        return Err(error);
    }
    let bytes = serde_json::to_vec_pretty(&next)?;
    let write = (|| {
        ensure!(config_bytes(&path)? == original, "Configuration changed during lockscreen setup");
        std::fs::create_dir_all(path.parent().context("Missing config directory")?)?;
        skwd_config::atomic_write_mode(&path, &bytes, Some(CONFIG_MODE))?;
        Ok::<(), anyhow::Error>(())
    })();
    if let Err(error) = write {
        rollback.restore().with_context(|| format!("Save failed: {error:#}"))?;
        return Err(error);
    }
    state.reload_config();
    Ok(())
}

impl Rollback<'_> {
    fn restore(&self) -> Result<()> {
        use skwd_wall_core::infrastructure::{lock_screens, plasma};

        if self.target == LockScreenTarget::Plasma {
            plasma::sync_lock_screen_config(self.state, self.config, self.current).map(|_| ())
        } else {
            if let Some(sddm) = self.sddm {
                sddm.restore(self.root)?;
            }
            if self.image_attempted {
                export::restore(self.root, self.previous)?;
            }
            if self.mode_changed {
                lock_screens::sync(self.original, self.previous, self.root)
            } else {
                lock_screens::sync_target(self.original, self.previous, self.root, self.target)
            }
        }
    }
}

pub(crate) fn request_sync(state: &Arc<WallState>) {
    let _ = sender().send(Job::Sync(Arc::clone(state)));
}

pub(crate) fn request_set(state: &Arc<WallState>, request: LockScreenSetRequest) -> Result<()> {
    let (reply, receiver) = oneshot::channel();
    sender()
        .send(Job::Set(Arc::clone(state), request, reply))
        .map_err(|_| anyhow::anyhow!("Lockscreen worker stopped"))?;
    receiver.blocking_recv().context("Lockscreen worker stopped")?
}

pub(crate) fn request_follow_sync(state: &Arc<WallState>) {
    if state.config().plasma_lock_screen_mode() == "follow"
        || skwd_wall_core::infrastructure::lock_screens::follows(state.config().root())
    {
        request_sync(state);
    }
}
