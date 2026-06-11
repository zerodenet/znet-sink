//! Kernel communication layer.
//!
//! This module isolates all kernel IPC communication from the rest of
//! the GUI application. It is designed so that:
//!
//! 1. Adding a new kernel requires only a new sub-module (e.g. `kernel::clash`)
//!    implementing [`KernelAdapter`](adapter::KernelAdapter).
//! 2. The transport and protocol layers are kernel-agnostic and reusable.
//! 3. The module can be extracted into a standalone crate with minimal changes.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │  kernel::adapter (KernelAdapter trait)          │
//! ├────────────┬────────────────────────────────────┤
//! │ zero/      │  (future: clash/, singbox/, ...)   │
//! │  adapter   │                                    │
//! │  queries   │                                    │
//! │  commands  │                                    │
//! │  parsing   │                                    │
//! │  config    │                                    │
//! │  events    │                                    │
//! ├────────────┴────────────────────────────────────┤
//! │  kernel::protocol  (JSON-line IPC client)       │
//! ├──────────────────────────────────────────────────┤
//! │  kernel::transport (socket/pipe I/O)            │
//! └──────────────────────────────────────────────────┘
//! ```

pub mod adapter;
pub(crate) mod connection;
pub mod protocol;
pub mod transport;
pub mod zero;

pub use adapter::KernelAdapter;
