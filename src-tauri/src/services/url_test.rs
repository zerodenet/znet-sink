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
    let started_at = state
        .runtime_host()
        .process_status()
        .ok()
        .and_then(|process| process.started_at_unix_ms);

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
    // Core's historical releases start at 0.0.4. The reset release line
    // preserves current capabilities without reclassifying those old builds.
    let reset_minimum = semver::Version::new(0, 0, 1);
    let legacy_start = semver::Version::parse("0.0.4-0").unwrap();
    version >= minimum || (version >= reset_minimum && version < legacy_start)
}

/// Missing fields inherit client defaults; explicit zero remains authoritative.
pub fn apply_default_tolerance(config: &mut Value, tolerance_ms: u64) -> AppResult<usize> {
    apply_tolerance(config, tolerance_ms, false)
}

pub fn apply_tolerance(config: &mut Value, tolerance_ms: u64, force: bool) -> AppResult<usize> {
    let root = config
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("Zero config must be a JSON object"))?;
    let Some(groups) = root
        .get_mut("outbound_groups")
        .and_then(Value::as_array_mut)
    else {
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
        if !is_url_test {
            continue;
        }

        if !force && group.contains_key("tolerance_ms") {
            continue;
        }
        group.insert("tolerance_ms".to_string(), json!(tolerance_ms));
        applied += 1;
    }

    Ok(applied)
}

/// One validation boundary for settings, imports, composition and manual requests.
pub fn normalize_url(raw: &str) -> AppResult<String> {
    let url = reqwest::Url::parse(raw.trim())
        .map_err(|_| AppError::invalid_argument("公共测速地址必须是完整的 HTTP(S) URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::invalid_argument(
            "公共测速地址需要 HTTP(S) 协议和主机，不能包含账号密码或片段",
        ));
    }
    Ok(url.to_string())
}

#[cfg(feature = "tool-node-probe")]
pub fn configured_url(state: &AppState) -> AppResult<String> {
    let config = common::lock(state.app_config(), "app_config")?.clone();
    let source = crate::configuration::preferences::source(state)?;
    if !config.overrides.url_test {
        if let Some(value) = source.pointer("/runtime/latency_test_url") {
            return normalize_url(value.as_str().ok_or_else(|| {
                AppError::invalid_argument("runtime.latency_test_url must be a URL string")
            })?);
        }
    }
    normalize_url(&config.url_test.url)
}

pub fn apply_url(config: &mut Value, url: &str) -> AppResult<()> {
    apply_url_with_preference(config, url, false)
}

pub fn apply_url_with_preference(config: &mut Value, url: &str, force: bool) -> AppResult<()> {
    let url = normalize_url(url)?;
    let root = config
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("Zero config must be an object"))?;
    let runtime = root
        .entry("runtime")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("runtime must be an object"))?;
    if force || !runtime.contains_key("latency_test_url") {
        runtime.insert("latency_test_url".into(), json!(url));
    }
    let effective_url = runtime["latency_test_url"].clone();
    if let Some(groups) = root
        .get_mut("outbound_groups")
        .and_then(Value::as_array_mut)
    {
        for group in groups {
            let is_urltest = group
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| {
                    kind.eq_ignore_ascii_case("url_test") || kind.eq_ignore_ascii_case("urltest")
                });
            if is_urltest && (force || group.get("url").is_none()) {
                group["url"] = effective_url.clone();
            }
        }
    }
    Ok(())
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
        assert!(config["outbound_groups"][1].get("tolerance_ms").is_none());
    }

    #[test]
    fn explicit_profile_tolerance_including_zero_is_preserved() {
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
        assert!(version_supports_tolerance("0.0.1"));
        assert!(version_supports_tolerance("v0.0.1"));
        assert!(version_supports_tolerance("0.0.2-dev.202609080000"));
        assert!(!version_supports_tolerance("0.0.1-rc.1"));
        assert!(!version_supports_tolerance("0.0.4"));
        assert!(!version_supports_tolerance("invalid"));
        assert!(!version_supports_tolerance("0.0.16-dev.2"));
        assert!(version_supports_tolerance("0.0.16-dev.3"));
        assert!(version_supports_tolerance("0.0.16-rc.1"));
        assert!(version_supports_tolerance("0.0.16"));
        assert!(version_supports_tolerance("0.0.17-dev.1"));
    }
}
