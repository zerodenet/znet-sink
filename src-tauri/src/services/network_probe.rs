use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter};

use crate::errors::{AppError, AppResult};
use crate::models::app_config::default_network_probe_urls;

pub const HOST_NETWORK_CHANGED_EVENT: &str = "host-network:changed";

#[cfg(target_os = "windows")]
static HOST_NETWORK_MONITOR: std::sync::OnceLock<(usize, usize)> = std::sync::OnceLock::new();

#[cfg(target_os = "windows")]
static LAST_HOST_NETWORK_EVENT_MS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

pub fn emit_host_network_changed(app_handle: &AppHandle, reason: &str) {
    let _ = app_handle.emit(
        HOST_NETWORK_CHANGED_EVENT,
        serde_json::json!({
            "reason": reason,
            "occurredAtUnixMs": crate::services::common::now_unix_ms(),
        }),
    );
}

/// Start a process-lifetime watcher for Windows IP-interface changes. Mobile
/// hotspot/ICS activation changes an interface, so the GUI can immediately
/// refresh its sharing diagnostics even when the proxy was enabled first.
pub fn start_host_network_monitor(app_handle: &AppHandle) -> AppResult<()> {
    start_host_network_monitor_platform(app_handle)
}

#[cfg(target_os = "windows")]
fn start_host_network_monitor_platform(app_handle: &AppHandle) -> AppResult<()> {
    use std::ffi::c_void;

    use windows_sys::Win32::{
        Foundation::{ERROR_SUCCESS, HANDLE},
        NetworkManagement::IpHelper::{
            CancelMibChangeNotify2, NotifyIpInterfaceChange, MIB_IPINTERFACE_ROW,
            MIB_NOTIFICATION_TYPE,
        },
        Networking::WinSock::AF_UNSPEC,
    };

    if HOST_NETWORK_MONITOR.get().is_some() {
        return Ok(());
    }

    unsafe extern "system" fn interface_changed(
        context: *const c_void,
        _row: *const MIB_IPINTERFACE_ROW,
        _notification_type: MIB_NOTIFICATION_TYPE,
    ) {
        use std::sync::atomic::Ordering;

        if context.is_null() {
            return;
        }
        let now = crate::services::common::now_unix_ms();
        let previous = LAST_HOST_NETWORK_EVENT_MS.swap(now, Ordering::Relaxed);
        if now.saturating_sub(previous) < 1_500 {
            return;
        }
        let app_handle = unsafe { &*(context as *const AppHandle) };
        emit_host_network_changed(app_handle, "ip_interface.changed");
    }

    let context = Box::into_raw(Box::new(app_handle.clone()));
    let mut notification_handle: HANDLE = std::ptr::null_mut();
    let status = unsafe {
        NotifyIpInterfaceChange(
            AF_UNSPEC,
            Some(interface_changed),
            context.cast(),
            false,
            &mut notification_handle,
        )
    };
    if status != ERROR_SUCCESS {
        unsafe { drop(Box::from_raw(context)) };
        return Err(AppError::internal(format!(
            "failed to watch Windows IP interface changes: error {status}"
        )));
    }

    if HOST_NETWORK_MONITOR
        .set((notification_handle as usize, context as usize))
        .is_err()
    {
        unsafe {
            CancelMibChangeNotify2(notification_handle);
            drop(Box::from_raw(context));
        }
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn start_host_network_monitor_platform(_app_handle: &AppHandle) -> AppResult<()> {
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkProbeResult {
    pub ip: String,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub org: Option<String>,
    pub isp: Option<String>,
}

/// Detect the host machine's current public network environment.
///
/// This GUI-side check never injects the managed kernel's local proxy address.
/// The default HTTP client follows proxy variables inherited by this process;
/// without them, it uses the host's direct network path.
pub fn probe_local_network(probe_urls: &[String]) -> AppResult<NetworkProbeResult> {
    let probe_urls = if probe_urls.is_empty() {
        default_network_probe_urls()
    } else {
        probe_urls.to_vec()
    };

    try_probe_urls(&probe_urls)
}

fn try_probe_urls(probe_urls: &[String]) -> AppResult<NetworkProbeResult> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|error| AppError::internal(format!("failed to build HTTP client: {error}")))?;

    let mut failures = Vec::new();
    for url in probe_urls {
        match fetch_probe_result(&client, url) {
            Ok(result) => return Ok(result),
            Err(error) => failures.push(format!("{url}: {}", error.message)),
        }
    }

    Err(AppError::internal(failures.join("; ")))
}

fn fetch_probe_result(
    client: &reqwest::blocking::Client,
    url: &str,
) -> AppResult<NetworkProbeResult> {
    let response = client
        .get(url)
        .send()
        .map_err(|error| AppError::internal(format!("request failed: {error}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError::internal(format!(
            "unexpected HTTP status {status}"
        )));
    }

    let body = response
        .text()
        .map_err(|error| AppError::internal(format!("failed to read response body: {error}")))?;

    parse_probe_response(&body)
}

fn parse_probe_response(body: &str) -> AppResult<NetworkProbeResult> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| AppError::internal(format!("failed to parse JSON response: {error}")))?;

    let object = value
        .as_object()
        .ok_or_else(|| AppError::internal("probe response must be a JSON object"))?;

    let ip = first_string(object, &["query", "ip", "origin"])
        .map(normalize_ip)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::internal("probe response did not contain an IP field"))?;

    let org = first_string(object, &["org", "organization"]);
    let isp = first_string(object, &["isp"]).or_else(|| org.clone());

    Ok(NetworkProbeResult {
        ip,
        country: first_string(object, &["country", "country_name"]),
        region: first_string(object, &["regionName", "region", "region_name"]),
        city: first_string(object, &["city"]),
        org,
        isp,
    })
}

fn first_string(object: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        object.get(*key).and_then(|value| match value {
            Value::String(text) => {
                let trimmed = text.trim();
                (!trimmed.is_empty()).then_some(trimmed.to_string())
            }
            _ => None,
        })
    })
}

fn normalize_ip(value: String) -> String {
    value
        .split(',')
        .next()
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::parse_probe_response;

    #[test]
    fn parse_ip_api_shape() {
        let result = parse_probe_response(
            r#"{
                "query":"1.2.3.4",
                "country":"Japan",
                "regionName":"Tokyo",
                "city":"Tokyo",
                "org":"Example Org",
                "isp":"Example ISP"
            }"#,
        )
        .unwrap();

        assert_eq!(result.ip, "1.2.3.4");
        assert_eq!(result.country.as_deref(), Some("Japan"));
        assert_eq!(result.region.as_deref(), Some("Tokyo"));
        assert_eq!(result.isp.as_deref(), Some("Example ISP"));
    }

    #[test]
    fn parse_ipinfo_shape() {
        let result = parse_probe_response(
            r#"{
                "ip":"5.6.7.8",
                "country":"US",
                "region":"California",
                "city":"San Jose",
                "org":"Example ASN"
            }"#,
        )
        .unwrap();

        assert_eq!(result.ip, "5.6.7.8");
        assert_eq!(result.region.as_deref(), Some("California"));
        assert_eq!(result.org.as_deref(), Some("Example ASN"));
        assert_eq!(result.isp.as_deref(), Some("Example ASN"));
    }

    #[test]
    fn parse_httpbin_shape() {
        let result = parse_probe_response(r#"{"origin":"9.9.9.9, 10.10.10.10"}"#).unwrap();

        assert_eq!(result.ip, "9.9.9.9");
        assert!(result.country.is_none());
    }
}
