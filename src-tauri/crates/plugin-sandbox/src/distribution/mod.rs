//! Publisher-owned releases, signed packages and host-owned local installation.
pub mod directory;
pub mod marketplace;
pub mod package;
pub mod remote;
pub mod store;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const MAX_PACKAGE_BYTES: usize = 4 * 1024 * 1024;
