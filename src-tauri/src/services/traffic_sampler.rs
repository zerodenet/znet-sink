//! Runtime traffic bridge and traffic-ball lifecycle hooks.
//!
//! Zero already emits `stats.sampled` once per second over the shared
//! multiplexed IPC event stream. `gui_events` normalizes that event once and
//! passes the typed `GuiTrafficStats` into this module, so there is no second
//! kernel poll and no second JSON parse on the high-volume event path.
//!
//! The same sample drives the macOS / best-effort Linux tray rate and is
//! forwarded only to an active traffic-ball window.
//!
//! The traffic-ball WebView is created lazily from the static Tauri window
//! config. The normal application baseline therefore remains a single WebView;
//! the transparent 96x96 WebView exists only while the floating ball is in use.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, OnceLock,
};

use serde_json::json;
use tauri::{AppHandle, Emitter, Listener, Manager};

use crate::kernel::zero::{build_traffic_snapshot, TrafficSample};
use crate::models::gui_core::GuiTrafficStats;
use crate::state::app_state::AppState;

const CORE_PROCESS_EXITED_EVENT: &str = "core:process-exited";
const TRAFFIC_SAMPLE_EVENT: &str = "traffic.sampled";

const TRAFFIC_BALL_LABEL: &str = "traffic-ball";
const TRAFFIC_BALL_CREATE_REQUEST_EVENT: &str = "traffic-ball:create-request";
const TRAFFIC_BALL_READY_EVENT: &str = "traffic-ball:ready";
const TRAFFIC_BALL_DESTROY_REQUEST_EVENT: &str = "traffic-ball:destroy-request";

static TRAY_BASELINE: OnceLock<Mutex<Option<TrafficSample>>> = OnceLock::new();
static TRAFFIC_BALL_CREATING: AtomicBool = AtomicBool::new(false);

fn tray_baseline() -> &'static Mutex<Option<TrafficSample>> {
    TRAY_BASELINE.get_or_init(|| Mutex::new(None))
}

pub(crate) fn handle_stats_sample(
    app: &AppHandle,
    totals: &GuiTrafficStats,
    sampled_at_unix_ms: u64,
) {
    let current = TrafficSample {
        stats: totals.clone(),
        sampled_at_unix_ms,
    };

    // Tray rate uses a private baseline so one-off GUI snapshot queries cannot
    // disturb the menu-bar delta calculation.
    let rate_snapshot = {
        let mut baseline = tray_baseline()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let snapshot = build_traffic_snapshot(totals.clone(), baseline.as_ref(), sampled_at_unix_ms);
        *baseline = Some(current.clone());
        snapshot
    };

    if rate_snapshot.stable {
        update_tray_rate_title(
            app,
            rate_snapshot.rates.upload_bps,
            rate_snapshot.rates.download_bps,
        );
    } else {
        clear_tray_rate_title(app);
    }

    // Keep the existing one-off `gui_traffic_snapshot` seed path warm without
    // adding another kernel query loop. A newly-created ball can therefore get
    // a useful first-frame rate immediately.
    let state = app.state::<AppState>();
    if let Ok(mut sample) = state.traffic_sample().lock() {
        *sample = Some(current);
    }

    // Only the traffic-ball needs the legacy flat event. Avoid broadcasting a
    // duplicate event to every window when the ball does not exist.
    if app.get_webview_window(TRAFFIC_BALL_LABEL).is_some() {
        let _ = app.emit_to(TRAFFIC_BALL_LABEL, TRAFFIC_SAMPLE_EVENT, totals);
    }
}

pub(crate) fn clear_runtime_traffic_state(app: &AppHandle) {
    *tray_baseline()
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = None;

    let state = app.state::<AppState>();
    if let Ok(mut sample) = state.traffic_sample().lock() {
        *sample = None;
    }
    clear_tray_rate_title(app);
}

fn create_traffic_ball_window(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window(TRAFFIC_BALL_LABEL).is_some() {
        return Ok(());
    }

    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == TRAFFIC_BALL_LABEL)
        .ok_or_else(|| "traffic-ball window config is unavailable".to_string())?;

    tauri::WebviewWindowBuilder::from_config(app, config)
        .map_err(|error| format!("failed to prepare traffic-ball window: {error}"))?
        .build()
        .map_err(|error| format!("failed to create traffic-ball window: {error}"))?;

    Ok(())
}

fn emit_traffic_ball_ready(app: &AppHandle, result: Result<(), String>) {
    let payload = match result {
        Ok(()) => json!({ "ok": true }),
        Err(error) => json!({ "ok": false, "error": error }),
    };
    let _ = app.emit_to("main", TRAFFIC_BALL_READY_EVENT, payload);
}

fn install_traffic_ball_lifecycle(app_handle: &AppHandle) {
    let create_app = app_handle.clone();
    app_handle.listen(TRAFFIC_BALL_CREATE_REQUEST_EVENT, move |_| {
        if create_app.get_webview_window(TRAFFIC_BALL_LABEL).is_some() {
            emit_traffic_ball_ready(&create_app, Ok(()));
            return;
        }

        // Multiple callers can wait on the same ready event. Only one native
        // creation attempt is allowed at a time.
        if TRAFFIC_BALL_CREATING.swap(true, Ordering::AcqRel) {
            return;
        }

        // Tauri documents a WebView2 deadlock risk when creating a webview
        // window directly inside a synchronous event handler on Windows. Keep
        // creation on a dedicated thread and signal the main WebView when the
        // configured window has been built.
        let app = create_app.clone();
        let spawn_result = std::thread::Builder::new()
            .name("traffic-ball-create".to_string())
            .spawn(move || {
                let result = create_traffic_ball_window(&app);
                TRAFFIC_BALL_CREATING.store(false, Ordering::Release);
                emit_traffic_ball_ready(&app, result);
            });

        if let Err(error) = spawn_result {
            TRAFFIC_BALL_CREATING.store(false, Ordering::Release);
            emit_traffic_ball_ready(
                &create_app,
                Err(format!("failed to spawn traffic-ball creator: {error}")),
            );
        }
    });

    let destroy_app = app_handle.clone();
    app_handle.listen(TRAFFIC_BALL_DESTROY_REQUEST_EVENT, move |_| {
        if let Some(window) = destroy_app.get_webview_window(TRAFFIC_BALL_LABEL) {
            let _ = window.destroy();
        }
    });
}

/// Install event-driven traffic-ball lifecycle hooks.
///
/// Kept as `spawn` for the existing setup call, but this function does not
/// spawn a sampling loop.
pub fn spawn(app_handle: AppHandle) {
    install_traffic_ball_lifecycle(&app_handle);

    let exit_app = app_handle.clone();
    app_handle.listen(CORE_PROCESS_EXITED_EVENT, move |_| {
        clear_runtime_traffic_state(&exit_app);
    });
}

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
