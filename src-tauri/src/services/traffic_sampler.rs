//! Runtime traffic bridge and traffic-ball lifecycle hooks.
//!
//! Zero already emits `stats.sampled` once per second over the shared
//! multiplexed IPC event stream. `gui_events` normalizes that event once and
//! passes the typed `GuiTrafficStats` into this module, so there is no second
//! kernel poll and no second JSON parse on the high-volume event path.
//!
//! The same sample drives the macOS / best-effort Linux tray rate and is
//! forwarded to the overview and an active traffic-ball window. Missing pushes
//! trigger bounded recovery reads owned by the existing GUI event stream.
//!
//! The traffic-ball WebView is created lazily from the static Tauri window
//! config, then hidden and reused. Reusing the surface avoids racing a later
//! show request against asynchronous WebView destruction.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, OnceLock,
};

use serde_json::json;
use tauri::{AppHandle, Emitter, Listener, Manager};

use crate::kernel::zero::{build_traffic_snapshot, TrafficSample};
use crate::models::gui_core::GuiTrafficStats;
use crate::state::app_state::AppState;

mod recovery;
pub(crate) use recovery::Recovery;

const CORE_PROCESS_EXITED_EVENT: &str = "core:process-exited";
const TRAFFIC_RATE_SAMPLE_EVENT: &str = "traffic:rate-sampled";

const TRAFFIC_BALL_LABEL: &str = "traffic-ball";
const TRAFFIC_BALL_CREATE_REQUEST_EVENT: &str = "traffic-ball:create-request";
const TRAFFIC_BALL_READY_EVENT: &str = "traffic-ball:ready";

static TRAY_BASELINE: OnceLock<Mutex<RateBaseline>> = OnceLock::new();
static TRAFFIC_BALL_CREATING: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct RateBaseline {
    generation: u64,
    sample: Option<TrafficSample>,
}

impl RateBaseline {
    fn next(
        &mut self,
        generation: u64,
        totals: &GuiTrafficStats,
        sampled_at: u64,
    ) -> Option<crate::models::gui_core::GuiTrafficSnapshot> {
        if self.generation != generation {
            self.generation = generation;
            self.sample = None;
        }
        rate_sample(&mut self.sample, totals, sampled_at)
    }
}

fn tray_baseline() -> &'static Mutex<RateBaseline> {
    TRAY_BASELINE.get_or_init(|| Mutex::new(RateBaseline::default()))
}

pub(crate) fn handle_stats_sample(
    app: &AppHandle,
    totals: &GuiTrafficStats,
    sampled_at_unix_ms: u64,
    generation: u64,
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
        if !app
            .state::<AppState>()
            .observations()
            .is_current(generation)
        {
            return;
        }
        let Some(snapshot) = baseline.next(generation, totals, sampled_at_unix_ms) else {
            return;
        };
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

    // The overview and traffic ball must display the exact same rate sample.
    // Compute it once in Rust and send the flattened result to the two UI
    // surfaces instead of letting each WebView maintain its own delta clock.
    let payload = json!({
        "generation": generation,
        "uploadBytesPerSec": rate_snapshot.rates.upload_bps,
        "downloadBytesPerSec": rate_snapshot.rates.download_bps,
        "totalUploadBytes": rate_snapshot.totals.bytes_up,
        "totalDownloadBytes": rate_snapshot.totals.bytes_down,
        "connectionCount": rate_snapshot.totals.active_sessions,
        "sampledAtUnixMs": rate_snapshot.sampled_at_unix_ms,
        "stable": rate_snapshot.stable,
    });
    let _ = app.emit_to("main", TRAFFIC_RATE_SAMPLE_EVENT, &payload);
    if app.get_webview_window(TRAFFIC_BALL_LABEL).is_some() {
        let _ = app.emit_to(TRAFFIC_BALL_LABEL, TRAFFIC_RATE_SAMPLE_EVENT, payload);
    }
}

fn rate_sample(
    baseline: &mut Option<TrafficSample>,
    totals: &GuiTrafficStats,
    sampled_at_unix_ms: u64,
) -> Option<crate::models::gui_core::GuiTrafficSnapshot> {
    if baseline.as_ref().is_some_and(|prev| {
        sampled_at_unix_ms <= prev.sampled_at_unix_ms
            || sampled_at_unix_ms - prev.sampled_at_unix_ms < 500
    }) {
        return None;
    }
    // A long gap is not a current speed. Re-establish the baseline, then
    // the next real sample restores the rate instead of averaging a pause.
    if baseline
        .as_ref()
        .is_some_and(|prev| sampled_at_unix_ms.saturating_sub(prev.sampled_at_unix_ms) > 10_000)
    {
        *baseline = None;
    }
    let snapshot = build_traffic_snapshot(totals.clone(), baseline.as_ref(), sampled_at_unix_ms);
    *baseline = Some(TrafficSample {
        stats: totals.clone(),
        sampled_at_unix_ms,
    });
    Some(snapshot)
}

pub(crate) fn clear_runtime_traffic_state(app: &AppHandle) {
    *tray_baseline()
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = RateBaseline::default();

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
        0 => "0K/s".to_string(),
        1..=999 => "<1K/s".to_string(),
        value if value < 1_000_000 => format!("{}/s", scaled(value, 1_000, 'K')),
        value if value < 1_000_000_000 => format!("{}/s", scaled(value, 1_000_000, 'M')),
        value => format!("{}/s", scaled(value, 1_000_000_000, 'G')),
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn update_tray_rate_title(app_handle: &AppHandle, upload_bps: u64, download_bps: u64) {
    if let Some(tray) = app_handle.tray_by_id("main-tray") {
        let title = format!(
            "↓ {}  ↑ {}",
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
        // tray-icon's macOS backend does not clear the native button title
        // when passed None, so use an explicit empty title instead.
        let _ = tray.set_title(Some(""));
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn clear_tray_rate_title(_app_handle: &AppHandle) {}

#[cfg(test)]
mod tests {
    use super::{compact_rate, rate_sample, RateBaseline};

    #[test]
    fn new_subscription_rebuilds_rates_from_its_own_counters() {
        use crate::models::gui_core::GuiTrafficStats;
        let stats = |bytes| GuiTrafficStats {
            bytes_down: bytes,
            ..Default::default()
        };
        let mut baseline = RateBaseline::default();
        assert!(!baseline.next(1, &stats(1000), 1000).unwrap().stable);
        assert!(baseline.next(1, &stats(2000), 2000).unwrap().stable);
        // A new peer can start within 500 ms and have lower counters. It must
        // not be dropped or measured against the previous peer's baseline.
        assert!(!baseline.next(2, &stats(10), 2100).unwrap().stable);
        let restored = baseline.next(2, &stats(5010), 3100).unwrap();
        assert!(restored.stable);
        assert_eq!(restored.rates.download_bps, 5000);
    }

    #[test]
    fn delayed_or_duplicate_samples_never_replace_current_rates() {
        use crate::models::gui_core::GuiTrafficStats;
        let stats = |bytes| GuiTrafficStats {
            bytes_down: bytes,
            ..Default::default()
        };
        let mut baseline = None;
        assert!(
            !rate_sample(&mut baseline, &stats(100), 1000)
                .unwrap()
                .stable
        );
        assert_eq!(
            rate_sample(&mut baseline, &stats(1100), 2000)
                .unwrap()
                .rates
                .download_bps,
            1000
        );
        assert!(rate_sample(&mut baseline, &stats(500), 1500).is_none());
        assert!(rate_sample(&mut baseline, &stats(1100), 2000).is_none());
        assert!(rate_sample(&mut baseline, &stats(1200), 2100).is_none());
        assert_eq!(
            rate_sample(&mut baseline, &stats(2100), 3000)
                .unwrap()
                .rates
                .download_bps,
            1000
        );
        assert!(
            !rate_sample(&mut baseline, &stats(9999), 20000)
                .unwrap()
                .stable
        );
        assert_eq!(
            rate_sample(&mut baseline, &stats(10999), 21000)
                .unwrap()
                .rates
                .download_bps,
            1000
        );
    }

    #[test]
    fn tray_rate_format_stays_compact_across_units() {
        assert_eq!(compact_rate(0), "0K/s");
        assert_eq!(compact_rate(512), "<1K/s");
        assert_eq!(compact_rate(1_000), "1.0K/s");
        assert_eq!(compact_rate(84_200), "84K/s");
        assert_eq!(compact_rate(1_250_000), "1.2M/s");
        assert_eq!(compact_rate(84_000_000), "84M/s");
        assert_eq!(compact_rate(1_250_000_000), "1.2G/s");
    }
}
