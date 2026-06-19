//! Zero kernel adapter module.
//!
//! Provides the [`ZeroAdapter`] implementing [`KernelAdapter`](crate::kernel::adapter::KernelAdapter)
//! for the self-developed Zero proxy engine.
//!
//! ## Module layout
//!
//! | File | Responsibility |
//! |------|---------------|
//! | `adapter.rs` | `ZeroAdapter` struct + `KernelAdapter` impl + traffic snapshot utilities |
//! | `queries.rs` | Async IPC query methods (health, stats, policies, connections, features) |
//! | `commands.rs` | Async IPC command methods (select_policy, probe, close, tun enable/disable) |
//! | `parsing.rs` | Pure JSON response parsing + utility functions |
//! | `config.rs` | Static config file parsing (no kernel required) |
//! | `events.rs` | Kernel event → GUI event normalization |

pub mod adapter;
pub mod commands;
pub mod config;
pub mod events;
pub mod parsing;
pub mod queries;

// Re-export the primary public API.
pub use adapter::{
    build_traffic_snapshot, bytes_delta_per_second, calculate_rates, TrafficSample, ZeroAdapter,
};
