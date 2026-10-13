use std::sync::{Mutex, RwLock, RwLockReadGuard};

use crate::config::Config;
use crate::lock;

pub struct ConfigStore {
    value: RwLock<Config>,
    source: Mutex<Vec<u8>>,
}

impl ConfigStore {
    pub fn load() -> Self {
        let (config, source) = Config::load_if_changed(&[])
            .unwrap_or_else(|| (Config::from_root(serde_json::Value::Null), Vec::new()));
        Self { value: RwLock::new(config), source: Mutex::new(source) }
    }

    pub fn read(&self) -> RwLockReadGuard<'_, Config> {
        self.value.read().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn reload_if_changed(&self) -> bool {
        let mut previous = lock(&self.source);
        let Some((config, source)) = Config::load_if_changed(&previous) else {
            return false;
        };
        *self.value.write().unwrap_or_else(std::sync::PoisonError::into_inner) = config;
        *previous = source;
        true
    }

    #[cfg(test)]
    pub(crate) fn from_root(root: serde_json::Value) -> Self {
        Self { value: RwLock::new(Config::from_root(root)), source: Mutex::new(Vec::new()) }
    }
}
