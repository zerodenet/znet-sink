use serde_json::{json, Value};

use crate::errors::{AppError, AppResult};
use crate::services::{common, kernel_manager};
use crate::state::app_state::AppState;

const URLTEST_TOLERANCE_FEATURE: &str = "urltest_tolerance";
const URLTEST_TOLERANCE_MIN_VERSION: &str = "0.0.16-dev.3";

/// Whether the configured/running Zero understands `tolerance_ms`.
///
/// A capability snapshot from the current process is authoritative. Before a
/// fresh process has been queried, fall back to the installed binary version
/// so startup config export remains compatible with older kernels.
pub fn supports_tolerance(state: &AppState) -> bool {
    let started_at = common::lock(state.core_process(), "core_process")
        .ok()
        .and_then(|process| process.status.started_at_unix_ms);

    if let Ok(cache) = common::lock(state.zero_features_cache(), "zero_features_cache") {
        if let Some(cached) = cache.as_ref() {
            let belongs_to_current_process = started_at
                .map(|started_at| cached.cached_at_unix_ms >= started_at)
                .unwrap_or(false);
            if belongs_to_current_process {
                return cached
                    .features
                    .iter()
                    .any(|feature| feature == URLTEST_TOLERANCE_FEATURE);
            }
        }
    }

    let core = match common::lock(state.app_config(), "app_config") {
        Ok(config) => config.core.clone(),
        Err(_) => return false,
    };
    kernel_manager::detect_installed_version(&core)
        .ok()
        .and_then(|detected| detected.version)
        .is_some_and(|version| version_supports_tolerance(&version))
}

fn version_supports_tolerance(version: &str) -> bool {
    let Ok(version) = semver::Version::parse(version.trim_start_matches('v')) else {
        return false;
    };
    let minimum = semver::Version::parse(URLTEST_TOLERANCE_MIN_VERSION)
        .expect("URLTest tolerance minimum version is valid semver");
    version >= minimum
}

/// Apply the client product default to URLTest groups that did not explicitly
/// configure a tolerance. The base profile is never mutated; this runs only on
/// the effective configuration passed to Zero.
///
/// Explicit values, including `0`, always win over the client default.
pub fn apply_default_tolerance(config: &mut Value, tolerance_ms: u64) -> AppResult<usize> {
    let root = config
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("Zero config must be a JSON object"))?;
    let Some(groups) = root.get_mut("outbound_groups").and_then(Value::as_array_mut) else {
        return Ok(0);
    };

    let mut applied = 0usize;
    for group in groups {
        let Some(group) = group.as_object_mut() else {
            continue;
        };
        let is_url_test = group
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|kind| {
                kind.eq_ignore_ascii_case("url_test") || kind.eq_ignore_ascii_case("urltest")
            });
        if !is_url_test || group.contains_key("tolerance_ms") {
            continue;
        }

        group.insert("tolerance_ms".to_string(), json!(tolerance_ms));
        applied += 1;
    }

    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::{apply_default_tolerance, version_supports_tolerance};
    use serde_json::json;

    #[test]
    fn missing_urltest_tolerance_gets_client_default() {
        let mut config = json!({
            "outbound_groups": [
                {
                    "tag": "Auto",
                    "type": "url_test",
                    "outbounds": ["HK", "US"]
                },
                {
                    "tag": "Select",
                    "type": "selector",
                    "outbounds": ["HK", "US"]
                }
            ]
        });

        assert_eq!(apply_default_tolerance(&mut config, 50).unwrap(), 1);
        assert_eq!(config["outbound_groups"][0]["tolerance_ms"], 50);
        assert!(config["outbound_groups"][1]
            .get("tolerance_ms")
            .is_none());
    }

    #[test]
    fn explicit_tolerance_including_zero_is_preserved() {
        let mut config = json!({
            "outbound_groups": [
                {"tag": "Strict", "type": "url_test", "outbounds": ["HK"], "tolerance_ms": 0},
                {"tag": "Sticky", "type": "url_test", "outbounds": ["US"], "tolerance_ms": 120}
            ]
        });

        assert_eq!(apply_default_tolerance(&mut config, 50).unwrap(), 0);
        assert_eq!(config["outbound_groups"][0]["tolerance_ms"], 0);
        assert_eq!(config["outbound_groups"][1]["tolerance_ms"], 120);
    }

    #[test]
    fn tolerance_version_gate_starts_at_dev3() {
        assert!(!version_supports_tolerance("0.0.16-dev.2"));
        assert!(version_supports_tolerance("0.0.16-dev.3"));
        assert!(version_supports_tolerance("0.0.16-rc.1"));
        assert!(version_supports_tolerance("0.0.16"));
        assert!(version_supports_tolerance("0.0.17-dev.1"));
    }
}
