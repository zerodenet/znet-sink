//! Publisher-owned releases, signed packages and host-owned local installation.
pub mod directory;
pub mod marketplace;
pub mod package;
pub mod remote;
pub mod store;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
/// Compressed package limit. Legacy JSON payloads retain their smaller limits,
/// while v3 application packages may use the extra space for signed assets.
pub const MAX_PACKAGE_BYTES: usize = 16 * 1024 * 1024;
