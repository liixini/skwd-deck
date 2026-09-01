pub mod repository;
#[cfg(feature = "daemon")]
mod store;

#[cfg(feature = "daemon")]
pub use store::Database;

#[cfg(all(test, feature = "daemon"))]
mod tests;
