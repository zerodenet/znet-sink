#[path = "subscription.rs"]
mod original;

pub use original::{ParsedSubscriptionConfig, SyncAllOutcome};

use base64::{engine::general_purpose, Engine as _};
use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use crate::errors::{AppError, AppResult};
use crate::models::subscription::{
    SubscriptionProfile, SubscriptionRemovalOutcome, SubscriptionRemovalPreview, SubscriptionUpsert,
};
use crate::services::{common::lock, domain_store};
use crate::state::app_state::AppState;

const CLIENT_USER_AGENT: &str = concat!("ZNet-Sink/", env!("CARGO_PKG_VERSION"));
const CLIENT_USER_AGENT_PREFIX: &str = "znet-sink/";

fn is_blank(value: Option<&str>) -> bool {
    match value {
        None => true,
        Some(value) => value.trim().is_empty(),
    }
}

fn is_zero_alias(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "zero"
            | "zero-json"
            | "zero-base64-json"
            | "base64-json"
            | "znet-sink"
            | "znet-sink-base64"
    )
}

fn is_clash_alias(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "clash" | "clash-yaml" | "yaml" | "clash-base64-yaml" | "base64-yaml"
    )
}

fn storage_format(value: Option<&str>) -> String {
    let value = value.unwrap_or_default().trim().to_ascii_lowercase();
    match value.as_str() {
        "" | "auto" => "auto".to_string(),
        value if is_zero_alias(value) => "zero-base64-json".to_string(),
        "clash-base64-yaml" | "base64-yaml" => "clash-base64-yaml".to_string(),
        value if is_clash_alias(value) => "clash-yaml".to_string(),
        value => value.to_string(),
    }
}

fn public_format(value: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() || value == "auto" {
        "auto".to_string()
    } else if is_zero_alias(&value) {
        "zero".to_string()
    } else if is_clash_alias(&value) {
        "clash".to_string()
    } else {
        value
    }
}

fn is_default_user_agent(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    value.split_whitespace().count() == 1 && value.starts_with(CLIENT_USER_AGENT_PREFIX)
}

fn strip_legacy_appended_client_identity(value: &str) -> String {
    let mut tokens = value.split_whitespace().collect::<Vec<_>>();
    if tokens.len() > 1
        && tokens.last().is_some_and(|token| {
            token
                .to_ascii_lowercase()
                .starts_with(CLIENT_USER_AGENT_PREFIX)
        })
    {
        tokens.pop();
    }
    tokens.join(" ")
}

fn effective_user_agent(value: Option<&str>) -> String {
    let value = value.unwrap_or_default().trim();
    if value.is_empty() {
        CLIENT_USER_AGENT.to_string()
    } else {
        value.to_string()
    }
}

fn normalize_stored_user_agent(value: Option<&str>) -> String {
    let value = value.unwrap_or_default().trim();
    if value.is_empty() || is_default_user_agent(value) {
        CLIENT_USER_AGENT.to_string()
    } else {
        strip_legacy_appended_client_identity(value)
    }
}

fn public_user_agent(value: Option<&str>) -> Option<String> {
    let value = value.unwrap_or_default().trim();
    if value.is_empty() || is_default_user_agent(value) {
        None
    } else {
        Some(strip_legacy_appended_client_identity(value))
    }
}

fn present_profile(mut profile: SubscriptionProfile) -> SubscriptionProfile {
    profile.format = public_format(&profile.format);
    profile.user_agent = public_user_agent(profile.user_agent.as_deref());
    profile
}

fn normalize_upsert(mut input: SubscriptionUpsert) -> SubscriptionUpsert {
    input.format = Some(storage_format(input.format.as_deref()));
    input.user_agent = Some(effective_user_agent(input.user_agent.as_deref()));
    input
}

fn migrate_profiles(state: &AppState) -> AppResult<()> {
    let mut subscriptions = lock(state.subscriptions(), "subscription")?;
    let mut next = subscriptions.clone();
    let mut changed = false;

    for profile in &mut next {
        let legacy_forced_auto = profile.format.trim().eq_ignore_ascii_case("zero-json")
            && is_blank(profile.user_agent.as_deref());
        let next_format = if legacy_forced_auto {
            "auto".to_string()
        } else {
            storage_format(Some(&profile.format))
        };
        let next_user_agent = normalize_stored_user_agent(profile.user_agent.as_deref());

        if profile.format != next_format {
            profile.format = next_format;
            changed = true;
        }
        if profile.user_agent.as_deref() != Some(next_user_agent.as_str()) {
            profile.user_agent = Some(next_user_agent);
            changed = true;
        }
    }

    if !changed {
        return Ok(());
    }

    domain_store::save_subscriptions(&next)?;
    *subscriptions = next;
    Ok(())
}

pub fn list(state: State<'_, AppState>) -> AppResult<Vec<SubscriptionProfile>> {
    migrate_profiles(state.inner())?;
    original::list(state).map(|items| items.into_iter().map(present_profile).collect())
}

pub fn get(state: State<'_, AppState>, id: String) -> AppResult<SubscriptionProfile> {
    migrate_profiles(state.inner())?;
    original::get(state, id).map(present_profile)
}

pub fn upsert(
    state: State<'_, AppState>,
    input: SubscriptionUpsert,
) -> AppResult<SubscriptionProfile> {
    migrate_profiles(state.inner())?;
    original::upsert(state, normalize_upsert(input)).map(present_profile)
}

pub async fn sync(app_handle: AppHandle, id: String) -> AppResult<SubscriptionProfile> {
    {
        let state = app_handle.state::<AppState>();
        migrate_profiles(state.inner())?;
    }
    original::sync(app_handle, id).await.map(present_profile)
}

pub async fn sync_all(app_handle: AppHandle) -> AppResult<SyncAllOutcome> {
    {
        let state = app_handle.state::<AppState>();
        migrate_profiles(state.inner())?;
    }
    original::sync_all(app_handle).await
}

pub fn removal_preview(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<SubscriptionRemovalPreview> {
    original::removal_preview(state, id)
}

pub async fn remove(
    app_handle: AppHandle,
    id: String,
    remove_associated_config: bool,
) -> AppResult<SubscriptionRemovalOutcome> {
    original::remove(app_handle, id, remove_associated_config).await
}

pub fn spawn_auto_sync_scheduler(app: AppHandle) {
    let state = app.state::<AppState>();
    if let Err(error) = migrate_profiles(state.inner()) {
        crate::services::file_logger::line(&format!(
            "subscription: failed to normalize stored profiles: {}",
            error.message
        ));
    }
    original::spawn_auto_sync_scheduler(app);
}

pub fn parse_subscription_content(
    content: &str,
    format: &str,
) -> AppResult<ParsedSubscriptionConfig> {
    let format = public_format(format);
    let mut parsed = match format.as_str() {
        "zero" => original::parse_subscription_content(content, "zero-base64-json"),
        "clash" => original::parse_subscription_content(content, "clash-yaml")
            .or_else(|_| original::parse_subscription_content(content, "clash-base64-yaml")),
        "auto" => {
            if let Ok(value) = serde_json::from_str::<Value>(content.trim()) {
                if looks_like_zero_config(&value) {
                    return Err(AppError::invalid_argument(
                        "明文 Zero JSON 订阅已不再支持，请使用 Base64 编码的 Zero JSON（格式：zero）",
                    ));
                }
            }
            original::parse_subscription_content(content, "auto")
        }
        other => original::parse_subscription_content(content, other),
    }?;

    let source_is_clash = parsed.format.contains("clash");
    if source_is_clash {
        apply_clash_urltest_tolerances(content, &mut parsed.content);
    }
    parsed.format = if source_is_clash {
        "clash".to_string()
    } else {
        "zero".to_string()
    };
    Ok(parsed)
}

fn apply_clash_urltest_tolerances(source_content: &str, converted: &mut Value) {
    let Some(source) = parse_clash_source(source_content) else {
        return;
    };
    let Some(source_groups) = source.get("proxy-groups").and_then(Value::as_array) else {
        return;
    };
    let Some(converted_groups) = converted
        .get_mut("outbound_groups")
        .and_then(Value::as_array_mut)
    else {
        return;
    };

    for source_group in source_groups {
        let Some(group) = source_group.as_object() else {
            continue;
        };
        let is_url_test = group
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind.eq_ignore_ascii_case("url-test"));
        if !is_url_test {
            continue;
        }
        let Some(name) = group.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(tolerance_ms) = group.get("tolerance").and_then(value_as_u64) else {
            continue;
        };

        if let Some(converted_group) = converted_groups.iter_mut().find(|candidate| {
            candidate.get("tag").and_then(Value::as_str) == Some(name)
                && candidate
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("url_test"))
        }) {
            if let Some(object) = converted_group.as_object_mut() {
                object
                    .entry("tolerance_ms".to_string())
                    .or_insert_with(|| Value::from(tolerance_ms));
            }
        }
    }
}

fn parse_clash_source(content: &str) -> Option<Value> {
    if let Ok(value) = serde_yaml::from_str::<Value>(content) {
        if value.is_object() {
            return Some(value);
        }
    }

    let compact = content.split_whitespace().collect::<String>();
    if compact.is_empty() {
        return None;
    }
    let mut padded = compact;
    let remainder = padded.len() % 4;
    if remainder != 0 {
        padded.extend(std::iter::repeat_n('=', 4 - remainder));
    }
    let decoded = general_purpose::STANDARD
        .decode(&padded)
        .or_else(|_| general_purpose::URL_SAFE.decode(&padded))
        .ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    serde_yaml::from_str::<Value>(&decoded)
        .ok()
        .filter(Value::is_object)
}

fn value_as_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|raw| raw.trim().parse().ok()))
}

fn looks_like_zero_config(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    [
        "outbounds",
        "outbound_groups",
        "route",
        "inbounds",
        "dns",
        "policy_groups",
        "policies",
    ]
    .iter()
    .any(|key| object.contains_key(*key))
}

#[cfg(test)]
mod wrapper_tests {
    use super::*;

    #[test]
    fn source_formats_are_canonicalized_without_rewriting_auto() {
        assert_eq!(storage_format(Some("auto")), "auto");
        assert_eq!(storage_format(Some("zero")), "zero-base64-json");
        assert_eq!(storage_format(Some("zero-json")), "zero-base64-json");
        assert_eq!(storage_format(Some("clash")), "clash-yaml");
        assert_eq!(public_format("zero-base64-json"), "zero");
        assert_eq!(public_format("clash-base64-yaml"), "clash");
    }

    #[test]
    fn user_agent_defaults_when_blank_and_custom_values_override() {
        assert_eq!(effective_user_agent(None), CLIENT_USER_AGENT);
        assert_eq!(
            effective_user_agent(Some("CustomClient/1.0")),
            "CustomClient/1.0"
        );
        assert_eq!(public_user_agent(Some(CLIENT_USER_AGENT)), None);
        assert_eq!(public_user_agent(Some("ZNet-Sink/0.0.1")), None);
        assert_eq!(
            normalize_stored_user_agent(Some("CustomClient/1.0 ZNet-Sink/0.0.1")),
            "CustomClient/1.0"
        );
        assert_eq!(
            public_user_agent(Some("CustomClient/1.0")),
            Some("CustomClient/1.0".to_string())
        );
    }

    #[test]
    fn zero_requires_base64_and_returns_canonical_format() {
        let raw = r#"{"outbounds":[{"tag":"direct","protocol":{"type":"direct"}}]}"#;
        let error = parse_subscription_content(raw, "zero").unwrap_err();
        assert_eq!(error.code, "invalid_argument");

        let encoded = general_purpose::STANDARD.encode(raw.as_bytes());
        let parsed = parse_subscription_content(&encoded, "zero").unwrap();
        assert_eq!(parsed.format, "zero");
    }

    #[test]
    fn auto_rejects_plaintext_zero_but_detects_encoded_zero() {
        let raw = r#"{"outbounds":[{"tag":"direct","protocol":{"type":"direct"}}]}"#;
        let error = parse_subscription_content(raw, "auto").unwrap_err();
        assert!(error.message.contains("明文 Zero JSON"));

        let encoded = general_purpose::STANDARD.encode(raw.as_bytes());
        let parsed = parse_subscription_content(&encoded, "auto").unwrap();
        assert_eq!(parsed.format, "zero");
    }

    #[test]
    fn clash_accepts_raw_and_base64_yaml_under_one_name() {
        let yaml = "proxies:\n  - {name: x, type: ss, server: s, port: 1, password: p}\n";
        assert_eq!(
            parse_subscription_content(yaml, "clash").unwrap().format,
            "clash"
        );
        let encoded = general_purpose::STANDARD.encode(yaml.as_bytes());
        assert_eq!(
            parse_subscription_content(&encoded, "clash")
                .unwrap()
                .format,
            "clash"
        );
    }

    #[test]
    fn clash_urltest_tolerance_is_preserved_for_raw_base64_and_auto_sources() {
        let yaml = "\
proxies:
  - {name: HK, type: ss, server: s, port: 1, password: p}
  - {name: JP, type: ss, server: s, port: 2, password: p}
proxy-groups:
  - {name: Auto, type: url-test, proxies: [HK, JP], url: http://x, interval: 300, tolerance: 75}
";

        let raw = parse_subscription_content(yaml, "clash").unwrap();
        assert_eq!(raw.content["outbound_groups"][0]["tolerance_ms"], 75);

        let encoded = general_purpose::STANDARD.encode(yaml.as_bytes());
        let base64 = parse_subscription_content(&encoded, "clash").unwrap();
        assert_eq!(base64.content["outbound_groups"][0]["tolerance_ms"], 75);

        let auto = parse_subscription_content(&encoded, "auto").unwrap();
        assert_eq!(auto.content["outbound_groups"][0]["tolerance_ms"], 75);
    }
}
