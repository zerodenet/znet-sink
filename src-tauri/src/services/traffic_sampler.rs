//! Background traffic sampler.
//!
//! Queries the kernel for cumulative traffic stats every second and emits a
//! `traffic.sampled` event so the overview chart updates live. Without this,
//! the frontend has no way to see traffic move: the only feed for the
//! overview is the `traffic.sampled` event, and the kernel itself does not
//! push it (see TODO P5).
//!
//! Stops as soon as the app begins shutting down ([`AppState::is_shutting_down`]).

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::kernel::adapter::KernelAdapter;
use crate::kernel::zero::{TrafficSample, ZeroAdapter};
use crate::models::core_process::CoreProcessState;
use crate::services::{common, core_config, core_process};
use crate::state::app_state::AppState;

/// Sampling cadence. ~1s keeps the overview chart smooth without flooding IPC.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

/// Spawn the background traffic sampler thread. Runs until shutdown.
pub fn spawn(app_handle: AppHandle) {
    std::thread::spawn(move || {
        let state = app_handle.state::<AppState>();
        loop {
            std::thread::sleep(SAMPLE_INTERVAL);
            if state.is_shutting_down() {
                return;
            }

            // Only sample while the kernel is actually running — otherwise
            // this just errors (and logs) every second.
            let running = matches!(
                core_process::refresh_status(state.inner()),
                Ok(status) if status.state == CoreProcessState::Running
            );
            if !running {
                continue;
            }

            let opts = core_config::ipc_options_from_app_config(
                &common::lock(state.app_config(), "app_config")
                    .map(|c| c.core.clone())
                    .unwrap_or_default(),
            );
            // traffic_stats is async; drive it from this dedicated thread.
            let totals = match tauri::async_runtime::block_on(async {
                ZeroAdapter::new().traffic_stats(opts).await
            }) {
                Ok(t) => t,
                Err(_) => continue,
            };

            // Persist the sample so the next iteration (or a one-off
            // `gui_traffic_snapshot` command) can compute rates.
            let sampled_at_unix_ms = common::now_unix_ms();
            if let Ok(mut sample) = state.traffic_sample().lock() {
                *sample = Some(TrafficSample {
                    stats: totals.clone(),
                    sampled_at_unix_ms,
                });
            }

            // Emit cumulative stats. The serialized keys (bytesUp/bytesDown
            // and bytes_up/bytes_down, plus activeSessions) are exactly what
            // `overviewData.applyStatsEvent` reads off the `traffic.sampled`
            // event.
            let _ = app_handle.emit("traffic.sampled", &totals);
        }
    });
}
