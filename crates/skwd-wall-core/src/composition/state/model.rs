use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLockReadGuard};

use rusqlite::Connection;

use crate::backend::wallpaper::ApplyRuntime;
use crate::config::Config;
use crate::infrastructure::config::ConfigStore;
use crate::infrastructure::database::Database;
use crate::infrastructure::renderers::RendererSupervisor;
use crate::lock;

const SHELL_PALETTE_CACHE_CAP: usize = 128;

/// Composition root for independently owned core services.
pub struct WallState {
    config: Arc<ConfigStore>,
    database: Arc<Database>,
    renderers: Arc<RendererSupervisor>,
    apply: Arc<ApplyRuntime>,
    scanner: ScannerRuntime,
    theme: ThemeRuntime,
}

#[derive(Default)]
pub struct ScannerRuntime {
    scanner_pid: AtomicU32,
}

#[derive(Default)]
pub struct ThemeRuntime {
    theme_source: Mutex<Option<String>>,
    noctalia_preview: Mutex<Option<(String, String)>>,
    bridge_preview: Mutex<Option<Vec<u8>>>,
    preview_files: Mutex<Vec<(std::path::PathBuf, Vec<u8>)>>,
    shell_preview_lock: Mutex<()>,
    shell_generation: AtomicU64,
    shell_palettes: Mutex<HashMap<String, Vec<u8>>>,
}

impl WallState {
    pub fn open() -> anyhow::Result<Self> {
        Ok(Self::from_components(
            Arc::new(ConfigStore::load()),
            Arc::new(Database::open()?),
            Arc::new(RendererSupervisor::default()),
            Arc::new(ApplyRuntime::default()),
        ))
    }

    fn from_components(
        config: Arc<ConfigStore>,
        database: Arc<Database>,
        renderers: Arc<RendererSupervisor>,
        apply: Arc<ApplyRuntime>,
    ) -> Self {
        Self {
            config,
            database,
            renderers,
            apply,
            scanner: ScannerRuntime::default(),
            theme: ThemeRuntime::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn test_new(root: serde_json::Value) -> Self {
        Self::from_components(
            Arc::new(ConfigStore::from_root(root)),
            Arc::new(Database::in_memory()),
            Arc::new(RendererSupervisor::default()),
            Arc::new(ApplyRuntime::default()),
        )
    }

    pub fn config_store(&self) -> Arc<ConfigStore> {
        Arc::clone(&self.config)
    }

    pub fn database(&self) -> Arc<Database> {
        Arc::clone(&self.database)
    }

    pub fn renderer_supervisor(&self) -> Arc<RendererSupervisor> {
        Arc::clone(&self.renderers)
    }

    pub fn apply_runtime(&self) -> Arc<ApplyRuntime> {
        Arc::clone(&self.apply)
    }

    pub fn renderers(&self) -> &RendererSupervisor {
        &self.renderers
    }

    pub fn renderers_shared(&self) -> Arc<RendererSupervisor> {
        Arc::clone(&self.renderers)
    }

    pub fn apply(&self) -> &ApplyRuntime {
        &self.apply
    }

    pub fn scanner(&self) -> &ScannerRuntime {
        &self.scanner
    }

    pub fn theme(&self) -> &ThemeRuntime {
        &self.theme
    }

    pub fn config(&self) -> RwLockReadGuard<'_, Config> {
        self.config.read()
    }

    pub fn reload_config(&self) {
        self.config.reload_if_changed();
    }

    pub fn with_db<T>(
        &self,
        operation: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> rusqlite::Result<T> {
        self.database.with_connection(operation)
    }
}

impl ThemeRuntime {
    pub fn lock_shell_preview(&self) -> std::sync::MutexGuard<'_, ()> {
        lock(&self.shell_preview_lock)
    }

    pub fn bump_shell_preview(&self) -> u64 {
        self.shell_generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn shell_preview_generation(&self) -> u64 {
        self.shell_generation.load(Ordering::SeqCst)
    }

    pub fn noctalia_preview_orig(&self) -> Option<(String, String)> {
        lock(&self.noctalia_preview).clone()
    }

    pub fn set_noctalia_preview_orig(&self, original: (String, String)) {
        *lock(&self.noctalia_preview) = Some(original);
    }

    pub fn take_noctalia_preview_orig(&self) -> Option<(String, String)> {
        lock(&self.noctalia_preview).take()
    }

    pub fn bridge_preview_armed(&self) -> bool {
        lock(&self.bridge_preview).is_some()
    }

    pub fn arm_bridge_preview(&self, original: Vec<u8>) {
        let mut preview = lock(&self.bridge_preview);
        if preview.is_none() {
            *preview = Some(original);
        }
    }

    pub fn take_bridge_preview(&self) -> Option<Vec<u8>> {
        lock(&self.bridge_preview).take()
    }

    pub fn arm_preview_files(&self, files: Vec<(std::path::PathBuf, Vec<u8>)>) {
        let mut preview = lock(&self.preview_files);
        if preview.is_empty() {
            *preview = files;
        }
    }

    pub fn take_preview_files(&self) -> Vec<(std::path::PathBuf, Vec<u8>)> {
        std::mem::take(&mut *lock(&self.preview_files))
    }

    pub fn shell_palette_cached(&self, key: &str) -> Option<Vec<u8>> {
        lock(&self.shell_palettes).get(key).cloned()
    }

    pub fn cache_shell_palette(&self, key: String, json: Vec<u8>) {
        let mut palettes = lock(&self.shell_palettes);
        if palettes.len() >= SHELL_PALETTE_CACHE_CAP {
            palettes.clear();
        }
        palettes.insert(key, json);
    }

    pub fn set_source(&self, path: &str) {
        *lock(&self.theme_source) = Some(path.to_string());
    }

    pub fn clear_source_if(&self, path: &str) {
        let mut source = lock(&self.theme_source);
        if source.as_deref() == Some(path) {
            *source = None;
        }
    }

    pub fn source(&self) -> Option<String> {
        lock(&self.theme_source).clone()
    }
}

impl ScannerRuntime {
    pub fn set_scanner_pid(&self, pid: u32) {
        self.scanner_pid.store(pid, Ordering::Relaxed);
    }

    pub fn scanner_rss_mb(&self) -> u64 {
        match self.scanner_pid.load(Ordering::Relaxed) {
            0 => 0,
            pid => crate::diag::pid_rss_kb(pid) / 1024,
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
