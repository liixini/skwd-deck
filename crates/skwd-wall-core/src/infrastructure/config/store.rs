use std::sync::{Mutex, RwLock, RwLockReadGuard};
use std::time::SystemTime;

use crate::config::Config;
use crate::lock;

pub struct ConfigStore {
    value: RwLock<Config>,
    mtime: Mutex<Option<SystemTime>>,
}

impl ConfigStore {
    pub fn load() -> Self {
        Self { value: RwLock::new(Config::load()), mtime: Mutex::new(Config::current_mtime()) }
    }

    pub fn read(&self) -> RwLockReadGuard<'_, Config> {
        self.value.read().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn reload_if_changed(&self) -> bool {
        let previous = *lock(&self.mtime);
        let Some((config, mtime)) = Config::load_if_changed(previous) else {
            return false;
        };
        *self.value.write().unwrap_or_else(std::sync::PoisonError::into_inner) = config;
        *lock(&self.mtime) = Some(mtime);
        true
    }

    #[cfg(test)]
    pub(crate) fn from_root(root: serde_json::Value) -> Self {
        Self { value: RwLock::new(Config::from_root(root)), mtime: Mutex::new(None) }
    }
}
