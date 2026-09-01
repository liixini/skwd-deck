use std::sync::Mutex;

use rusqlite::Connection;

use crate::db;
use crate::lock;

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn open() -> anyhow::Result<Self> {
        Ok(Self { connection: Mutex::new(db::open()?) })
    }

    pub fn with_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> rusqlite::Result<T> {
        let connection = lock(&self.connection);
        operation(&connection)
    }

    #[cfg(test)]
    pub(crate) fn in_memory() -> Self {
        Self { connection: Mutex::new(db::open_in_memory().expect("in-memory database")) }
    }
}
