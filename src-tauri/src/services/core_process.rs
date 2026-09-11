//! Compatibility entrypoints; runtime ownership lives in runtime_host.
pub(crate) use crate::runtime_host::{refresh_status, stop_preserving_system_proxy};
pub use crate::runtime_host::{restart, shutdown_managed_runtime, start, status, stop};
