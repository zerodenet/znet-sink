use std::time::Duration;
use tauri::State;

use base64::{engine::general_purpose, Engine as _};

use crate::errors::{AppError, AppResult};
use crate::models::proxy_config::ProxyConfigProfile;
use crate::models::subscription::{SubscriptionProfile, SubscriptionUpsert};
use crate::services::common::{
    generated_store_id, lock, normalize_optional, normalize_required, now_unix_ms,
};
use crate::services::domain_store;
use crate::services::proxy_config;
use crate::state::app_state::AppState;

const SUBSCRIPTION_FETCH_TIMEOUT_SECONDS: u64 = 30;

pub fn list(state: State<'_, AppState>) -> AppResult<Vec<SubscriptionProfile>> {
    Ok(lock(state.subscriptions(), "subscription")?.clone())
}

pub fn get(state: State<'_, AppState>, id: String) -> AppResult<SubscriptionProfile> {
    let id = normalize_required(id, "id")?;
    lock(state.subscriptions(), "subscription")?
        .iter()
        .find(|profile| profile.id == id)
        .cloned()
        .ok_or_else(|| AppError::not_found("subscription", id))
}

pub fn upsert(
    state: State<'_, AppState>,
    input: SubscriptionUpsert,
) -> AppResult<SubscriptionProfile> {
    let name = normalize_required(input.name, "name")?;
    let url = normalize_required(input.url, "url")?;
    validate_http_url(&url)?;

    let id = normalize_optional(input.id).unwrap_or_else(|| generated_store_id("subscription"));
    let profile = SubscriptionProfile {
        id: id.clone(),
        name,
        url,
        enabled: input.enabled.unwrap_or(true),
        kernel: normalize_optional(input.kernel).unwrap_or_else(|| "zero".to_string()),
        format: normalize_optional(input.format).unwrap_or_else(|| "auto".to_string()),
        target_proxy_config_id: normalize_optional(input.target_proxy_config_id),
        updated_at_unix_ms: now_unix_ms(),
        last_sync_at_unix_ms: None,
        last_error: None,
    };

    let mut subscriptions = lock(state.subscriptions(), "subscription")?;
    match subscriptions.iter_mut().find(|item| item.id == id) {
        Some(existing) => {
            let last_sync_at_unix_ms = existing.last_sync_at_unix_ms;
            *existing = SubscriptionProfile {
                last_sync_at_unix_ms,
                ..profile.clone()
            };
        }
        None => subscriptions.push(profile.clone()),
    }
    domain_store::save_subscriptions(&subscriptions)?;

    Ok(profile)
}

pub async fn sync(state: State<'_, AppState>, id: String) -> AppResult<SubscriptionProfile> {
    let id = normalize_required(id, "id")?;
    let subscription = {
        let subscriptions = lock(state.subscriptions(), "subscription")?;
        subscriptions
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
            .ok_or_else(|| AppError::not_found("subscription", id.clone()))?
    };

    if !subscription.enabled {
        let error = AppError::invalid_argument("subscription is disabled");
        update_sync_error(state.inner(), &id, &error.message)?;
        return Err(error);
    }

    let result = sync_subscription(state.inner(), subscription).await;
    if let Err(error) = &result {
        update_sync_error(state.inner(), &id, &error.message)?;
    }

    result
}

pub fn remove(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let id = normalize_required(id, "id")?;
    let mut subscriptions = lock(state.subscriptions(), "subscription")?;
    let before = subscriptions.len();
    subscriptions.retain(|profile| profile.id != id);

    if subscriptions.len() == before {
        return Err(AppError::not_found("subscription", id));
    }
    domain_store::save_subscriptions(&subscriptions)?;

    Ok(())
}

fn validate_http_url(url: &str) -> AppResult<()> {
    if url.starts_with("https://") || url.starts_with("http://") {
        return Ok(());
    }

    Err(AppError::invalid_argument(
        "subscription url must start with http:// or https://",
    ))
}

async fn sync_subscription(
    state: &AppState,
    subscription: SubscriptionProfile,
) -> AppResult<SubscriptionProfile> {
    let content = fetch_subscription_content(subscription.url.clone()).await?;
    let parsed = parse_subscription_content(&content, &subscription.format)?;
    let now = now_unix_ms();
    let target_proxy_config_id = subscription
        .target_proxy_config_id
        .clone()
        .unwrap_or_else(|| generated_store_id("proxy-config"));

    upsert_synced_proxy_config(state, &subscription, &target_proxy_config_id, parsed, now)?;
    update_sync_success(state, &subscription.id, target_proxy_config_id, now)
}

async fn fetch_subscription_content(url: String) -> AppResult<String> {
    tauri::async_runtime::spawn_blocking(move || fetch_subscription_content_blocking(&url))
        .await
        .map_err(|error| AppError::internal(format!("subscription worker failed: {error}")))?
}

fn fetch_subscription_content_blocking(url: &str) -> AppResult<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(SUBSCRIPTION_FETCH_TIMEOUT_SECONDS))
        .user_agent("ZNet Sink")
        .build()
        .map_err(|error| AppError::internal(format!("failed to build HTTP client: {error}")))?;

    let response = client.get(url).send().map_err(|error| AppError {
        code: "upstream_error",
        message: format!("failed to fetch subscription: {error}"),
        details: Some(serde_json::json!({ "url": url })),
    })?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError {
            code: "upstream_error",
            message: format!("subscription server returned HTTP {status}"),
            details: Some(serde_json::json!({ "url": url, "status": status.as_u16() })),
        });
    }

    response.text().map_err(|error| AppError {
        code: "upstream_error",
        message: format!("failed to read subscription response: {error}"),
        details: Some(serde_json::json!({ "url": url })),
    })
}

#[derive(Clone, Debug)]
pub struct ParsedSubscriptionConfig {
    pub content: serde_json::Value,
    pub format: String,
}

pub fn parse_subscription_content(
    content: &str,
    format: &str,
) -> AppResult<ParsedSubscriptionConfig> {
    let content = content.trim();
    if content.is_empty() {
        return Err(AppError::invalid_argument(
            "subscription response must not be empty",
        ));
    }

    let format = format.trim().to_ascii_lowercase();
    match format.as_str() {
        "" | "auto" | "zero" | "zero-base64-json" | "base64-json" => {
            parse_base64_json_subscription_content(content)
        }
        _ => Err(AppError::invalid_argument(format!(
            "unsupported subscription format: {format}"
        ))),
    }
}

fn parse_base64_json_subscription_content(content: &str) -> AppResult<ParsedSubscriptionConfig> {
    let decoded = decode_base64(content)?;
    let decoded = String::from_utf8(decoded).map_err(|error| AppError {
        code: "invalid_argument",
        message: format!("subscription decoded content is not valid UTF-8: {error}"),
        details: None,
    })?;

    let content: serde_json::Value = serde_json::from_str(&decoded).map_err(|error| AppError {
        code: "invalid_argument",
        message: format!("subscription decoded JSON is invalid: {error}"),
        details: None,
    })?;
    if !content.is_object() {
        return Err(AppError::invalid_argument(
            "subscription decoded JSON must be an object",
        ));
    }

    Ok(ParsedSubscriptionConfig {
        content,
        format: "zero-base64-json".to_string(),
    })
}

fn decode_base64(content: &str) -> AppResult<Vec<u8>> {
    let compact = content.split_whitespace().collect::<String>();
    if compact.is_empty() {
        return Err(AppError::invalid_argument(
            "subscription response must not be empty",
        ));
    }

    let padded = pad_base64(&compact);
    general_purpose::STANDARD
        .decode(&padded)
        .or_else(|_| general_purpose::URL_SAFE.decode(&padded))
        .map_err(|error| AppError {
            code: "invalid_argument",
            message: format!("subscription response must be base64 encoded JSON: {error}"),
            details: None,
        })
}

fn pad_base64(content: &str) -> String {
    let mut padded = content.to_string();
    let remainder = padded.len() % 4;
    if remainder != 0 {
        padded.extend(std::iter::repeat_n('=', 4 - remainder));
    }
    padded
}

fn upsert_synced_proxy_config(
    state: &AppState,
    subscription: &SubscriptionProfile,
    target_proxy_config_id: &str,
    parsed: ParsedSubscriptionConfig,
    updated_at_unix_ms: u64,
) -> AppResult<ProxyConfigProfile> {
    let capabilities = proxy_config::analyze_capabilities(Some(&parsed.content));
    let mut profiles = lock(state.proxy_configs(), "proxy_config")?;
    let existing_active = profiles
        .iter()
        .find(|profile| profile.id == target_proxy_config_id)
        .is_some_and(|profile| profile.active);
    let profile = ProxyConfigProfile {
        id: target_proxy_config_id.to_string(),
        name: subscription.name.clone(),
        kernel: subscription.kernel.clone(),
        format: parsed.format,
        path: Some(subscription.url.clone()),
        content: Some(parsed.content),
        active: existing_active,
        updated_at_unix_ms,
        capabilities,
    };

    match profiles
        .iter_mut()
        .find(|profile| profile.id == target_proxy_config_id)
    {
        Some(existing) => *existing = profile.clone(),
        None => profiles.push(profile.clone()),
    }
    domain_store::save_proxy_configs(&profiles)?;
    if profile.active {
        proxy_config::sync_local_proxy_from_profile(state, &profile)?;
    }

    Ok(profile)
}

fn update_sync_success(
    state: &AppState,
    id: &str,
    target_proxy_config_id: String,
    synced_at_unix_ms: u64,
) -> AppResult<SubscriptionProfile> {
    let mut subscriptions = lock(state.subscriptions(), "subscription")?;
    let subscription = subscriptions
        .iter_mut()
        .find(|profile| profile.id == id)
        .ok_or_else(|| AppError::not_found("subscription", id.to_string()))?;

    subscription.target_proxy_config_id = Some(target_proxy_config_id);
    subscription.last_sync_at_unix_ms = Some(synced_at_unix_ms);
    subscription.last_error = None;
    subscription.updated_at_unix_ms = synced_at_unix_ms;
    let updated = subscription.clone();
    domain_store::save_subscriptions(&subscriptions)?;

    Ok(updated)
}

fn update_sync_error(state: &AppState, id: &str, message: &str) -> AppResult<()> {
    let mut subscriptions = lock(state.subscriptions(), "subscription")?;
    if let Some(subscription) = subscriptions.iter_mut().find(|profile| profile.id == id) {
        subscription.last_error = Some(message.to_string());
        subscription.updated_at_unix_ms = now_unix_ms();
        domain_store::save_subscriptions(&subscriptions)?;
    }

    Ok(())
}
