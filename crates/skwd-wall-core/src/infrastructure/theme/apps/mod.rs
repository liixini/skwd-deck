mod catalogue;
mod documents;
mod files;
mod manager;
mod plasma;
mod reload;
mod structured;

pub use manager::{apply, list, set_enabled};

#[cfg(test)]
mod tests;
