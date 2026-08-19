//! Background traffic sampler.
//!
//! Queries the kernel for cumulative traffic stats every second and emits a
//! `traffic.sampled` event so the overview chart updates live. Without this,
//! the frontend has no way to see traffic move: the only feed for the
//! overview is the `traffic.sampled` event, and the kernel itself does not
//! push it (see TODO P5).
//!
//! The same sample also drives the compact tray title on platforms that can
//! render text next to a tray/status icon (macOS and best-effort Linux). This
//! deliberately reuses the existing sampler instead of introducing another
//! polling loop just for desktop chrome.
//!
//! Stops as soon as the app begins shutting down ([`AppState::is_shutting_down`]).

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::kernel::adapter::KernelAdapter;
use crate::kernel::zero::{build_traffic_snapshot, TrafficSample, ZeroAdapter};
use crate::models::core_process::CoreProcessState;
use crate::services::{common, core_config, core_process};
use crate::state::app_state::AppState;

/// Sampling cadence. ~1s keeps the overview chart smooth without flooding IPC.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn compact_rate(bytes_per_second: u64) -> String {
    fn scaled(value: u64, unit: u64, suffix: char) -> String {
        let amount = value as f64 / unit as f64;
        if amount < 10.0 {
            format!("{amount:.1}{suffix}")
        } else {
            format!("{amount:.0}{suffix}")
        }
    }

    match bytes_per_second {
        0 => "0K".to_string(),
        1..=999 => "<1K".to_string(),
        value if value < 1_000_000 => scaled(value, 1_000, 'K'),
        value if value < 1_000_000_000 => scaled(value, 1_000_000, 'M'),
        value => scaled(value, 1_000_000_000, 'G'),
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn update_tray_rate_title(app_handle: &AppHandle, upload_bps: u64, download_bps: u64) {
    if let Some(tray) = app_handle.tray_by_id("main-tray") {
        let title = format!(
            "↓{} ↑{}",
            compact_rate(download_bps),
            compact_rate(upload_bps)
        );
        let _ = tray.set_title(Some(title));
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn update_tray_rate_title(_app_handle: &AppHandle, _upload_bps: u64, _download_bps: u64) {}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn clear_tray_rate_title(app_handle: &AppHandle) {
    if let Some(tray) = app_handle.tray_by_id("main-tray") {
        let _ = tray.set_title(None::<String>);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn clear_tray_rate_title(_app_handle: &AppHandle) {}

/// Spawn the background traffic sampler thread. Runs until shutdown.
pub fn spawn(app_handle: AppHandle) {
    std::thread::spawn(move || {
        let state = app_handle.state::<AppState>();
        // Keep the tray rate baseline private to this sampler. Commands such
        // as `gui_traffic_snapshot` also touch AppState's one-off baseline;
        // sharing that baseline here would make the menu-bar rate depend on
        // when a frontend request happened to run.
        let mut previous_rate_sample: Option<TrafficSample> = None;

        loop {
            std::thread::sleep(SAMPLE_INTERVAL);
            if state.is_shutting_down() {
                clear_tray_rate_title(&app_handle);
                return;
            }

            // Only sample while the kernel is actually running — otherwise
            // this just errors (and logs) every second.
            let running = matches!(
                core_process::refresh_status(state.inner()),
                Ok(status) if status.state == CoreProcessState::Running
            );
            if !running {
                previous_rate_sample = None;
                clear_tray_rate_title(&app_handle);
                continue;
            }

            let opts = core_config::ipc_options_from_app_config(
                &common::lock(state.app_config(), "app_config")
                    .map(|c| c.core.clone())
                    .unwrap_or_default(),
            );
            // traffic_stats is async; drive it from this dedicated thread.
            // Safe to block_on: this loop runs on a `std::thread::spawn`
            // thread, not a tokio worker, so there's no nested-runtime risk.
            let totals = match tauri::async_runtime::block_on(async {
                ZeroAdapter::new().traffic_stats(opts).await
            }) {
                Ok(t) => t,
                Err(_) => {
                    previous_rate_sample = None;
                    clear_tray_rate_title(&app_handle);
                    continue;
                }
            };

            let sampled_at_unix_ms = common::now_unix_ms();
            let rate_snapshot = build_traffic_snapshot(
                totals.clone(),
                previous_rate_sample.as_ref(),
                sampled_at_unix_ms,
            );
            let current_sample = TrafficSample {
                stats: totals.clone(),
                sampled_at_unix_ms,
            };
            previous_rate_sample = Some(current_sample.clone());

            if rate_snapshot.stable {
                update_tray_rate_title(
                    &app_handle,
                    rate_snapshot.rates.upload_bps,
                    rate_snapshot.rates.download_bps,
                );
            } else {
                // The first sample has no baseline. Prefer the icon by itself
                // for that first second instead of presenting a fake 0 K/s.
                clear_tray_rate_title(&app_handle);
            }

            // Persist the sample so a one-off `gui_traffic_snapshot` command
            // has a recent baseline available for its own response.
            if let Ok(mut sample) = state.traffic_sample().lock() {
                *sample = Some(current_sample);
            }

            // Emit cumulative stats. The serialized keys (bytesUp/bytesDown
            // and bytes_up/bytes_down, plus activeSessions) are exactly what
            // `overviewData.applyStatsEvent` reads off the `traffic.sampled`
            // event.
            let _ = app_handle.emit("traffic.sampled", &totals);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::compact_rate;

    #[test]
    fn tray_rate_format_stays_compact_across_units() {
        assert_eq!(compact_rate(0), "0K");
        assert_eq!(compact_rate(512), "<1K");
        assert_eq!(compact_rate(1_000), "1.0K");
        assert_eq!(compact_rate(84_200), "84K");
        assert_eq!(compact_rate(1_250_000), "1.2M");
        assert_eq!(compact_rate(84_000_000), "84M");
        assert_eq!(compact_rate(1_250_000_000), "1.2G");
    }
}
