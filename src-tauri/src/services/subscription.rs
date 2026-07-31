use std::{collections::HashMap, time::Duration};
use tauri::{AppHandle, Manager, State};

use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Map, Value};

use crate::errors::{AppError, AppResult};
use crate::models::logs::LogLevel;
use crate::models::proxy_config::{ProxyConfigProfile, ProxyConfigUpsert};
use crate::models::subscription::{SubscriptionProfile, SubscriptionUpsert, SyncMetadata};
use crate::services::common::{
    begin_in_flight, generated_store_id, is_in_flight, lock, normalize_optional,
    normalize_required, now_unix_ms,
};
use crate::services::{domain_store, logs, proxy_config, rule_set};
use crate::state::app_state::AppState;

const SUBSCRIPTION_FETCH_TIMEOUT_SECONDS: u64 = 30;
/// Default auto-sync check cadence for the background scheduler.
const AUTO_SYNC_TICK_SECONDS: u64 = 60;
/// Grace delay before the first auto-sync pass so the kernel and
/// networking stack have time to come up on startup.
const AUTO_SYNC_WARMUP_SECONDS: u64 = 15;
/// Number of exponential-backoff retries after the initial scheduled
/// attempt. A failed cycle therefore performs at most four requests.
const AUTO_SYNC_MAX_RETRIES: u32 = 3;
const AUTO_SYNC_RETRY_BASE_SECONDS: u64 = 60;
const AUTO_SYNC_RETRY_MAX_SECONDS: u64 = 15 * 60;

const DEFAULT_USER_AGENT: &str = concat!("ZNet-Sink/", env!("CARGO_PKG_VERSION"));
const DEFAULT_CLASH_USER_AGENT: &str = "Clash.Meta";

/// How often (in seconds) an auto-sync interval may be configured at
/// minimum. Prevents accidentally hammering a provider.
const MIN_AUTO_SYNC_INTERVAL_SECS: u64 = 60;

#[derive(Clone, Debug, PartialEq, Eq)]
struct AutoSyncRetryState {
    /// Failed attempts in the current cycle, including the initial attempt.
    failed_attempts: u32,
    next_attempt_at_unix_ms: u64,
    /// A successful manual sync changes this value and invalidates the
    /// scheduler's in-memory retry state on the next pass.
    cycle_last_sync_at_unix_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AutoSyncAttemptKind {
    Initial,
    Retry { number: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DueSubscription {
    id: String,
    name: String,
    interval_secs: u64,
    last_sync_at_unix_ms: Option<u64>,
    kind: AutoSyncAttemptKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AutoSyncFailureDisposition {
    RetryScheduled {
        retry_number: u32,
        delay_secs: u64,
        next_attempt_at_unix_ms: u64,
    },
    RetryLimitReached {
        cooldown_secs: u64,
        next_cycle_at_unix_ms: u64,
    },
}

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
    let _in_flight = begin_in_flight(state.subscription_syncs(), "subscription", &id)?;
    let kernel = normalize_optional(input.kernel).unwrap_or_else(|| "zero".to_string());
    let format = normalize_optional(input.format).unwrap_or_else(|| "auto".to_string());
    let update_interval_secs = validate_update_interval(input.update_interval_secs)?;
    let user_agent = normalize_optional(input.user_agent);
    let target_proxy_config_id = normalize_optional(input.target_proxy_config_id);

    let mut subscriptions = lock(state.subscriptions(), "subscription")?;
    let mut next = subscriptions.clone();
    let profile = match next.iter_mut().find(|item| item.id == id) {
        Some(existing) => {
            // Preserve sync-derived state across edits; only the
            // user-editable fields are overwritten.
            existing.name = name;
            existing.url = url;
            existing.kernel = kernel;
            existing.format = format;
            existing.update_interval_secs = update_interval_secs;
            existing.user_agent = user_agent;
            existing.target_proxy_config_id = target_proxy_config_id;
            existing.enabled = input.enabled.unwrap_or(existing.enabled);
            existing.updated_at_unix_ms = now_unix_ms();
            existing.clone()
        }
        None => {
            let profile = SubscriptionProfile {
                id: id.clone(),
                name,
                url,
                enabled: input.enabled.unwrap_or(true),
                kernel,
                format,
                target_proxy_config_id,
                policy_selections: Default::default(),
                update_interval_secs,
                user_agent,
                node_count: None,
                upload_bytes: None,
                download_bytes: None,
                total_bytes: None,
                expire_at_unix_ms: None,
                updated_at_unix_ms: now_unix_ms(),
                last_sync_at_unix_ms: None,
                last_error: None,
            };
            next.push(profile.clone());
            profile
        }
    };
    domain_store::save_subscriptions(&next)?;
    *subscriptions = next;

    Ok(profile)
}

pub async fn sync(app_handle: AppHandle, id: String) -> AppResult<SubscriptionProfile> {
    let id = normalize_required(id, "id")?;
    let state = app_handle.state::<AppState>();
    sync_by_id(&app_handle, state.inner(), &id).await
}

/// Sync every enabled subscription sequentially. Returns the number
/// that succeeded. Used by the UI's "sync all" action and by the
/// background auto-sync scheduler.
pub async fn sync_all(app_handle: AppHandle) -> AppResult<SyncAllOutcome> {
    let state = app_handle.state::<AppState>();
    sync_all_with_state(&app_handle, state.inner()).await
}

pub fn remove(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let id = normalize_required(id, "id")?;
    let _in_flight = begin_in_flight(state.subscription_syncs(), "subscription", &id)?;
    let mut subscriptions = lock(state.subscriptions(), "subscription")?;
    let mut next = subscriptions.clone();
    let before = next.len();
    next.retain(|profile| profile.id != id);

    if next.len() == before {
        return Err(AppError::not_found("subscription", id));
    }
    domain_store::save_subscriptions(&next)?;
    *subscriptions = next;

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

fn validate_update_interval(value: Option<u64>) -> AppResult<Option<u64>> {
    match value {
        None | Some(0) => Ok(None),
        Some(secs) if secs < MIN_AUTO_SYNC_INTERVAL_SECS => Err(AppError::invalid_argument(
            format!("update interval must be at least {MIN_AUTO_SYNC_INTERVAL_SECS} seconds"),
        )),
        Some(secs) => Ok(Some(secs)),
    }
}

/// Outcome of a batch sync, surfaced to the UI so it can report how
/// many subscriptions updated successfully.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncAllOutcome {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
}

async fn sync_by_id(
    app_handle: &AppHandle,
    state: &AppState,
    id: &str,
) -> AppResult<SubscriptionProfile> {
    let _in_flight = begin_in_flight(state.subscription_syncs(), "subscription", id)?;
    let subscription = {
        let subscriptions = lock(state.subscriptions(), "subscription")?;
        subscriptions
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
            .ok_or_else(|| AppError::not_found("subscription", id.to_string()))?
    };

    if !subscription.enabled {
        let error = AppError::invalid_argument("subscription is disabled");
        update_sync_error(state, id, &error.message)?;
        return Err(error);
    }

    let result = sync_subscription(app_handle, state, subscription).await;
    if let Err(error) = &result {
        update_sync_error(state, id, &error.message)?;
    }

    result
}

async fn sync_all_with_state(
    app_handle: &AppHandle,
    state: &AppState,
) -> AppResult<SyncAllOutcome> {
    let ids: Vec<String> = lock(state.subscriptions(), "subscription")?
        .iter()
        .filter(|profile| profile.enabled)
        .map(|profile| profile.id.clone())
        .collect();

    let mut succeeded = 0usize;
    for id in &ids {
        if sync_by_id(app_handle, state, id).await.is_ok() {
            succeeded += 1;
        }
    }

    Ok(SyncAllOutcome {
        total: ids.len(),
        succeeded,
        failed: ids.len().saturating_sub(succeeded),
    })
}

async fn sync_subscription(
    app_handle: &AppHandle,
    state: &AppState,
    subscription: SubscriptionProfile,
) -> AppResult<SubscriptionProfile> {
    let user_agent = subscription
        .user_agent
        .clone()
        .unwrap_or_else(|| default_user_agent_for_format(&subscription.format).to_string());
    let response = fetch_subscription_content(subscription.url.clone(), user_agent).await?;
    let mut parsed = parse_subscription_content(&response.content, &subscription.format)?;
    let now = now_unix_ms();
    let target_proxy_config_id = subscription
        .target_proxy_config_id
        .clone()
        .unwrap_or_else(|| generated_store_id("proxy-config"));

    let metadata = SyncMetadata {
        node_count: Some(count_proxy_nodes(&parsed.content)),
        upload_bytes: response.userinfo.upload,
        download_bytes: response.userinfo.download,
        total_bytes: response.userinfo.total,
        expire_at_unix_ms: response.userinfo.expire_ms(),
    };

    if parsed.format.contains("clash") {
        let sources = std::mem::take(&mut parsed.rule_providers)
            .into_iter()
            .map(|provider| rule_set::ManagedRuleSetSource {
                tag: provider.tag,
                url: provider.url,
                update_interval_secs: provider.update_interval_secs,
                user_agent: subscription.user_agent.clone(),
            })
            .collect();
        let outcome = rule_set::sync_managed_subscription_sources(
            state,
            &subscription.id,
            &subscription.name,
            sources,
        )
        .await?;
        let removed_tags = inject_synced_rule_sets(&mut parsed.content, outcome.artifacts)?;
        for failure in outcome.failures {
            logs::znet_log_fields(
                Some(state),
                LogLevel::Warn,
                format!(
                    "subscription rule provider '{}' failed: {}; {}",
                    failure.tag,
                    failure.message,
                    if failure.used_previous_artifact {
                        "continuing with the last verified ZRS"
                    } else {
                        "dropping its route rules"
                    }
                ),
                json!({
                    "schema": "znet.subscription-rule-provider.v1",
                    "operation": "sync",
                    "subscriptionId": subscription.id,
                    "subscriptionName": subscription.name,
                    "ruleSetTag": failure.tag,
                    "usedPreviousArtifact": failure.used_previous_artifact,
                    "routeReferencesRemoved": removed_tags.contains(&failure.tag),
                    "errorMessage": failure.message,
                }),
            );
        }
    }

    ensure_subscription_unchanged(state, &subscription)?;

    upsert_synced_proxy_config(
        app_handle,
        state,
        &subscription,
        &target_proxy_config_id,
        parsed,
    )
    .await?;
    update_sync_success(
        state,
        &subscription.id,
        target_proxy_config_id,
        metadata,
        now,
    )
}

fn default_user_agent_for_format(format: &str) -> &'static str {
    match format.trim().to_ascii_lowercase().as_str() {
        "" | "auto" | "clash" | "clash-yaml" | "yaml" | "clash-base64-yaml" | "base64-yaml" => {
            DEFAULT_CLASH_USER_AGENT
        }
        _ => DEFAULT_USER_AGENT,
    }
}

fn inject_synced_rule_sets(
    content: &mut Value,
    artifacts: Vec<rule_set::ManagedRuleSetArtifact>,
) -> AppResult<Vec<String>> {
    let route = content
        .get_mut("route")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| AppError::invalid_argument("converted subscription has no route object"))?;
    let available_tags = artifacts
        .iter()
        .map(|artifact| artifact.tag.clone())
        .collect::<std::collections::BTreeSet<_>>();
    route.insert(
        "rule_sets".to_string(),
        Value::Array(
            artifacts
                .into_iter()
                .map(|artifact| {
                    json!({
                        "tag": artifact.tag,
                        "type": "file",
                        "path": artifact.path,
                        "format": "zrs"
                    })
                })
                .collect(),
        ),
    );
    let mut removed_tags = std::collections::BTreeSet::new();
    if let Some(rules) = route.get_mut("rules").and_then(Value::as_array_mut) {
        rules.retain(|rule| {
            let referenced = rule
                .get("condition")
                .filter(|condition| {
                    condition.get("type").and_then(Value::as_str) == Some("rule_set")
                })
                .and_then(|condition| condition.get("tag"))
                .and_then(Value::as_str);
            let keep = referenced.is_none_or(|tag| available_tags.contains(tag));
            if !keep {
                removed_tags.insert(
                    referenced
                        .expect("missing rule-set tag is retained")
                        .to_string(),
                );
            }
            keep
        });
    }
    Ok(removed_tags.into_iter().collect())
}

fn ensure_subscription_unchanged(
    state: &AppState,
    snapshot: &SubscriptionProfile,
) -> AppResult<()> {
    let subscriptions = lock(state.subscriptions(), "subscription")?;
    let current = subscriptions
        .iter()
        .find(|profile| profile.id == snapshot.id)
        .ok_or_else(|| AppError::not_found("subscription", snapshot.id.clone()))?;
    if current.updated_at_unix_ms != snapshot.updated_at_unix_ms
        || current.url != snapshot.url
        || current.format != snapshot.format
        || current.target_proxy_config_id != snapshot.target_proxy_config_id
        || current.enabled != snapshot.enabled
    {
        return Err(AppError::conflict(
            "subscription",
            snapshot.id.clone(),
            "subscription changed while synchronization was in progress",
        ));
    }
    Ok(())
}

/// Raw response captured from the subscription endpoint, including
/// the optional `subscription-userinfo` header used to track traffic
/// usage and expiry.
#[derive(Clone, Debug)]
struct SubscriptionFetch {
    content: String,
    userinfo: SubscriptionUserinfo,
}

async fn fetch_subscription_content(
    url: String,
    user_agent: String,
) -> AppResult<SubscriptionFetch> {
    tauri::async_runtime::spawn_blocking(move || {
        fetch_subscription_content_blocking(&url, &user_agent)
    })
    .await
    .map_err(|error| AppError::internal(format!("subscription worker failed: {error}")))?
}

fn fetch_subscription_content_blocking(
    url: &str,
    user_agent: &str,
) -> AppResult<SubscriptionFetch> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(SUBSCRIPTION_FETCH_TIMEOUT_SECONDS))
        .user_agent(user_agent)
        // Use reqwest's default environment-variable proxy policy. The GUI
        // does not infer a download proxy from kernel or OS proxy state.
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

    let userinfo = response
        .headers()
        .get("subscription-userinfo")
        .and_then(|value| value.to_str().ok())
        .map(parse_subscription_userinfo)
        .unwrap_or_default();
    let content = response.text().map_err(|error| AppError {
        code: "upstream_error",
        message: format!("failed to read subscription response: {error}"),
        details: Some(serde_json::json!({ "url": url })),
    })?;
    Ok(SubscriptionFetch { content, userinfo })
}

/// Parsed `subscription-userinfo` header.
///
/// Format (clash convention):
/// `upload=NUM; download=NUM; total=NUM; expire=UNIX_SECONDS`
#[derive(Clone, Debug, Default)]
struct SubscriptionUserinfo {
    upload: Option<u64>,
    download: Option<u64>,
    total: Option<u64>,
    /// Expiry as a Unix timestamp in **seconds** (provider convention).
    expire_secs: Option<u64>,
}

impl SubscriptionUserinfo {
    /// Convert the header's seconds-based expiry to milliseconds,
    /// matching the rest of the model.
    fn expire_ms(&self) -> Option<u64> {
        self.expire_secs
            .filter(|secs| *secs > 0)
            .map(|secs| secs * 1000)
    }
}

fn parse_subscription_userinfo(header: &str) -> SubscriptionUserinfo {
    let mut info = SubscriptionUserinfo::default();
    for pair in header.split(';') {
        let pair = pair.trim();
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let Ok(value) = value.trim().parse::<u64>() else {
            continue;
        };
        match key {
            "upload" => info.upload = Some(value),
            "download" => info.download = Some(value),
            "total" => info.total = Some(value),
            "expire" => info.expire_secs = Some(value),
            _ => {}
        }
    }
    info
}

#[derive(Clone, Debug)]
pub struct ParsedSubscriptionConfig {
    pub content: serde_json::Value,
    pub format: String,
    rule_providers: Vec<ClashRuleProvider>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClashRuleProvider {
    tag: String,
    url: String,
    update_interval_secs: Option<u64>,
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
        "" | "auto" => parse_auto_subscription_content(content),
        "zero" | "zero-json" => parse_zero_json_subscription_content(content),
        "zero-base64-json" | "base64-json" => parse_base64_json_subscription_content(content),
        "clash" | "clash-yaml" | "yaml" => parse_clash_yaml_subscription_content(content),
        "clash-base64-yaml" | "base64-yaml" => {
            parse_base64_clash_yaml_subscription_content(content)
        }
        _ => Err(AppError::invalid_argument(format!(
            "unsupported subscription format: {format}"
        ))),
    }
}

fn parse_auto_subscription_content(content: &str) -> AppResult<ParsedSubscriptionConfig> {
    // 1. Raw Zero JSON (many providers serve plain JSON).
    if content.starts_with('{') || content.starts_with('[') {
        if let Ok(value) = serde_json::from_str::<Value>(content) {
            if looks_like_zero_config(&value) {
                return Ok(ParsedSubscriptionConfig {
                    content: value,
                    format: "zero-json".to_string(),
                    rule_providers: Vec::new(),
                });
            }
        }
    }

    // 2. Base64-encoded Zero JSON.
    if let Ok(parsed) = parse_base64_json_subscription_content(content) {
        return Ok(parsed);
    }

    // 3. Raw Clash YAML.
    if let Ok(parsed) = parse_clash_yaml_subscription_content(content) {
        return Ok(parsed);
    }

    // 4. Base64-encoded Clash YAML (common for clash subscriptions).
    if let Ok(parsed) = parse_base64_clash_yaml_subscription_content(content) {
        return Ok(parsed);
    }

    Err(AppError::invalid_argument(
        "subscription content did not match any supported format \
         (zero-json, zero-base64-json, clash-yaml, clash-base64-yaml)",
    ))
}

/// Heuristic: does this JSON object look like a Zero kernel config?
/// Avoids accepting arbitrary JSON in auto mode.
fn looks_like_zero_config(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    const KNOWN_KEYS: &[&str] = &[
        "outbounds",
        "outbound_groups",
        "route",
        "inbounds",
        "dns",
        "policy_groups",
        "policies",
    ];
    KNOWN_KEYS.iter().any(|key| object.contains_key(*key))
}

fn parse_zero_json_subscription_content(content: &str) -> AppResult<ParsedSubscriptionConfig> {
    let content: serde_json::Value = serde_json::from_str(content).map_err(|error| AppError {
        code: "invalid_argument",
        message: format!("subscription JSON is invalid: {error}"),
        details: None,
    })?;
    if !content.is_object() {
        return Err(AppError::invalid_argument(
            "subscription JSON must be an object",
        ));
    }

    Ok(ParsedSubscriptionConfig {
        content,
        format: "zero-json".to_string(),
        rule_providers: Vec::new(),
    })
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
        rule_providers: Vec::new(),
    })
}

fn parse_clash_yaml_subscription_content(content: &str) -> AppResult<ParsedSubscriptionConfig> {
    let clash: Value = serde_yaml::from_str(content).map_err(|error| AppError {
        code: "invalid_argument",
        message: format!("subscription Clash YAML is invalid: {error}"),
        details: None,
    })?;

    let (content, rule_providers) = convert_clash_to_zero(&clash)?;
    Ok(ParsedSubscriptionConfig {
        content,
        format: "clash-yaml-converted".to_string(),
        rule_providers,
    })
}

fn parse_base64_clash_yaml_subscription_content(
    content: &str,
) -> AppResult<ParsedSubscriptionConfig> {
    let decoded = decode_base64(content)?;
    let decoded = String::from_utf8(decoded).map_err(|error| AppError {
        code: "invalid_argument",
        message: format!("subscription decoded content is not valid UTF-8: {error}"),
        details: None,
    })?;

    let clash: Value = serde_yaml::from_str(&decoded).map_err(|error| AppError {
        code: "invalid_argument",
        message: format!("subscription decoded Clash YAML is invalid: {error}"),
        details: None,
    })?;

    let (content, rule_providers) = convert_clash_to_zero(&clash)?;
    Ok(ParsedSubscriptionConfig {
        content,
        format: "clash-base64-yaml-converted".to_string(),
        rule_providers,
    })
}

fn count_proxy_nodes(content: &Value) -> u32 {
    let Some(outbounds) = content.get("outbounds").and_then(Value::as_array) else {
        return 0;
    };
    outbounds
        .iter()
        .filter(|node| {
            let protocol = resolve_outbound_protocol(node);
            is_countable_proxy(protocol)
        })
        .count() as u32
}

/// Outbound types that represent a usable proxy node rather than a
/// special endpoint or policy group.
fn is_countable_proxy(protocol: &str) -> bool {
    const GROUPS_AND_SPECIAL: &[&str] = &[
        "direct",
        "block",
        "dns",
        "selector",
        "urltest",
        "url_test",
        "fallback",
        "loadbalance",
        "load_balance",
        "relay",
    ];
    !GROUPS_AND_SPECIAL
        .iter()
        .any(|kind| protocol.eq_ignore_ascii_case(kind))
}

fn resolve_outbound_protocol(node: &Value) -> &str {
    node.get("protocol")
        .and_then(|p| p.get("type").and_then(|v| v.as_str()))
        .or_else(|| node.get("type").and_then(|v| v.as_str()))
        .or_else(|| node.get("protocol").and_then(|v| v.as_str()))
        .unwrap_or("unknown")
}

fn convert_clash_to_zero(clash: &Value) -> AppResult<(Value, Vec<ClashRuleProvider>)> {
    let root = clash.as_object().ok_or_else(|| {
        AppError::invalid_argument("subscription Clash YAML root must be an object")
    })?;

    let proxies = root
        .get("proxies")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AppError::invalid_argument("subscription Clash YAML must contain proxies")
        })?;

    let mut outbounds = Vec::new();
    outbounds.push(json!({ "tag": "direct", "protocol": { "type": "direct" } }));
    outbounds.push(json!({ "tag": "block", "protocol": { "type": "block" } }));
    outbounds.extend(proxies.iter().filter_map(convert_clash_proxy));

    // Collect every resolvable tag up front — both proxy node tags and
    // policy group names — so a group may reference another group (nested
    // groups, e.g. a `select` group pointing at an `url-test` group).
    // Without this, the referenced group's tag is unknown while each group
    // is being converted, so intra-group references get dropped (and a
    // group that references only other groups disappears entirely).
    let mut known_tags = outbounds
        .iter()
        .filter_map(|outbound| outbound.get("tag").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect::<std::collections::BTreeSet<_>>();
    if let Some(groups) = root.get("proxy-groups").and_then(Value::as_array) {
        for group in groups {
            if let Some(name) = group.as_object().and_then(|o| string_field(o, "name")) {
                known_tags.insert(name);
            }
        }
    }

    let outbound_groups = root
        .get("proxy-groups")
        .and_then(Value::as_array)
        .map(|groups| {
            groups
                .iter()
                .filter_map(|group| convert_clash_proxy_group(group, &known_tags))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut rules = root
        .get("rules")
        .and_then(Value::as_array)
        .map(|rules| {
            rules
                .iter()
                .filter_map(|rule| convert_clash_rule(rule, &known_tags))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let referenced_rule_sets = rules
        .iter()
        .filter_map(|rule| {
            rule.get("condition")
                .and_then(|condition| condition.get("type"))
                .and_then(Value::as_str)
                .filter(|kind| *kind == "rule_set")
                .and_then(|_| rule.get("condition"))
                .and_then(|condition| condition.get("tag"))
                .and_then(Value::as_str)
        })
        .map(ToString::to_string)
        .collect::<std::collections::BTreeSet<_>>();
    let rule_providers = convert_clash_rule_providers(root, &referenced_rule_sets);
    let available_rule_sets = rule_providers
        .iter()
        .map(|provider| provider.tag.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    rules.retain(|rule| {
        rule.get("condition")
            .filter(|condition| condition.get("type").and_then(Value::as_str) == Some("rule_set"))
            .and_then(|condition| condition.get("tag"))
            .and_then(Value::as_str)
            .is_none_or(|tag| available_rule_sets.contains(tag))
    });

    let final_outbound = root
        .get("rules")
        .and_then(Value::as_array)
        .and_then(|rules| {
            rules
                .iter()
                .rev()
                .find_map(|rule| clash_match_outbound(rule, &known_tags))
        })
        .unwrap_or_else(|| {
            outbound_groups
                .first()
                .and_then(|group| group.get("tag").and_then(Value::as_str))
                .unwrap_or("direct")
                .to_string()
        });

    Ok((
        json!({
            "outbounds": outbounds,
            "outbound_groups": outbound_groups,
            "route": {
                "rule_sets": [],
                "rules": rules,
                "final": { "type": "route", "outbound": final_outbound }
            }
        }),
        rule_providers,
    ))
}

fn convert_clash_rule_providers(
    root: &Map<String, Value>,
    referenced: &std::collections::BTreeSet<String>,
) -> Vec<ClashRuleProvider> {
    let Some(providers) = root.get("rule-providers").and_then(Value::as_object) else {
        for tag in referenced {
            warn_skipped_clash_rule_provider(tag, "rule-providers is missing");
        }
        return Vec::new();
    };

    let mut converted = Vec::with_capacity(referenced.len());
    for tag in referenced {
        let Some(provider) = providers.get(tag).and_then(Value::as_object) else {
            warn_skipped_clash_rule_provider(tag, "no matching rule-provider definition");
            continue;
        };
        let provider_type = string_field(provider, "type")
            .unwrap_or_else(|| "http".to_string())
            .to_ascii_lowercase();
        if provider_type != "http" {
            warn_skipped_clash_rule_provider(
                tag,
                &format!("unsupported provider type '{provider_type}'"),
            );
            continue;
        }
        let behavior = string_field(provider, "behavior")
            .unwrap_or_else(|| "classical".to_string())
            .to_ascii_lowercase();
        if behavior != "classical" {
            warn_skipped_clash_rule_provider(tag, &format!("unsupported behavior '{behavior}'"));
            continue;
        }
        let Some(url) = string_field(provider, "url") else {
            warn_skipped_clash_rule_provider(tag, "provider URL is missing");
            continue;
        };
        let update_interval_secs = provider
            .get("interval")
            .and_then(|value| {
                value
                    .as_u64()
                    .or_else(|| value.as_str().and_then(|raw| raw.parse().ok()))
            })
            .filter(|seconds| *seconds > 0);
        converted.push(ClashRuleProvider {
            tag: tag.to_string(),
            url: normalize_clash_rule_provider_url(&url),
            update_interval_secs,
        });
    }
    converted
}

fn warn_skipped_clash_rule_provider(tag: &str, reason: &str) {
    crate::services::file_logger::emit(
        "warn",
        "subscription",
        "subscription.rule_provider.definition_skipped",
        Some(json!({
            "ruleSetTag": tag,
            "reason": reason,
        })),
    );
}

fn normalize_clash_rule_provider_url(url: &str) -> String {
    // Older dler.io Clash templates used a path-compatible mirror for GitHub
    // raw content. That endpoint now returns 404 while the repository and the
    // same path remain available from GitHub's canonical raw origin.
    const DLER_RAW_PREFIX: &str = "https://raw.dler.io/";
    url.strip_prefix(DLER_RAW_PREFIX)
        .map(|path| format!("https://raw.githubusercontent.com/{path}"))
        .unwrap_or_else(|| url.to_string())
}

fn convert_clash_proxy(proxy: &Value) -> Option<Value> {
    let source = proxy.as_object()?;
    let tag = string_field(source, "name")?;
    let proxy_type = string_field(source, "type")?.to_ascii_lowercase();

    // Build the nested `protocol` object per Zero's strict serde schema.
    // Each protocol accepts only a specific field set; clash-only fields
    // (udp, alterId, tfo, …) and clash aliases (skip-cert-verify, uuid, …)
    // are normalized or dropped in the builders. See:
    // https://docs.zerodenet.org/project/config#outbounds
    let protocol = match proxy_type.as_str() {
        "ss" | "shadowsocks" => build_shadowsocks(source)?,
        "ssr" => build_shadowsocksr(source)?,
        "vmess" => build_vmess(source)?,
        "vless" => build_vless(source)?,
        "trojan" => build_trojan(source)?,
        "socks5" | "socks" => build_socks(source)?,
        "http" | "https" => build_http(source)?,
        "hysteria2" | "hysteria" => build_hysteria2(source)?,
        other => {
            // Unknown protocol — emit clash's own type tag so the kernel can
            // surface a clear "unsupported protocol" error instead of the node
            // silently disappearing from the outbound list.
            let mut p = Map::new();
            p.insert("type".to_string(), Value::String(other.to_string()));
            p
        }
    };

    let mut outbound = Map::new();
    outbound.insert("tag".to_string(), Value::String(tag));
    outbound.insert("protocol".to_string(), Value::Object(protocol));
    Some(Value::Object(outbound))
}

// ── per-protocol builders ──
//
// Each builder returns the inner `protocol` object. `server_port` and the
// TLS/transport helpers below normalize clash's flat fields into the shape
// Zero's serde expects.

fn server_port(source: &Map<String, Value>) -> Option<(String, u64)> {
    let server = string_field(source, "server")?;
    let port = source.get("port").and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })?;
    Some((server, port))
}

fn build_shadowsocks(s: &Map<String, Value>) -> Option<Map<String, Value>> {
    let (server, port) = server_port(s)?;
    let password = string_field(s, "password")?;
    let cipher = string_field(s, "cipher").unwrap_or_else(|| "chacha20-ietf-poly1305".to_string());
    let mut p = Map::new();
    p.insert("type".to_string(), json!("shadowsocks"));
    p.insert("server".to_string(), json!(server));
    p.insert("port".to_string(), json!(port));
    p.insert("password".to_string(), json!(password));
    p.insert("cipher".to_string(), json!(cipher));
    Some(p)
}

fn build_shadowsocksr(s: &Map<String, Value>) -> Option<Map<String, Value>> {
    let (server, port) = server_port(s)?;
    let password = string_field(s, "password")?;
    let cipher = string_field(s, "cipher").unwrap_or_else(|| "aes-256-cfb".to_string());
    let mut p = Map::new();
    p.insert("type".to_string(), json!("shadowsocksr"));
    p.insert("server".to_string(), json!(server));
    p.insert("port".to_string(), json!(port));
    p.insert("password".to_string(), json!(password));
    p.insert("cipher".to_string(), json!(cipher));
    if let Some(obfs) = string_field(s, "obfs") {
        p.insert("obfs".to_string(), json!(obfs));
    }
    if let Some(protocol) = string_field(s, "protocol") {
        p.insert("protocol".to_string(), json!(protocol));
    }
    if let Some(param) =
        string_field(s, "protocol-param").or_else(|| string_field(s, "protocol_param"))
    {
        p.insert("protocol_param".to_string(), json!(param));
    }
    Some(p)
}

fn build_trojan(s: &Map<String, Value>) -> Option<Map<String, Value>> {
    let (server, port) = server_port(s)?;
    let password = string_field(s, "password")?;
    let mut p = Map::new();
    p.insert("type".to_string(), json!("trojan"));
    p.insert("server".to_string(), json!(server));
    p.insert("port".to_string(), json!(port));
    p.insert("password".to_string(), json!(password));
    if let Some(sni) = resolve_sni(s) {
        p.insert("sni".to_string(), json!(sni));
    }
    if let Some(insecure) = resolve_insecure(s) {
        p.insert("insecure".to_string(), json!(insecure));
    }
    if let Some(fp) = resolve_client_fingerprint(s) {
        p.insert("client_fingerprint".to_string(), json!(fp));
    }
    Some(p)
}

fn build_vmess(s: &Map<String, Value>) -> Option<Map<String, Value>> {
    let (server, port) = server_port(s)?;
    let id = string_field(s, "uuid").or_else(|| string_field(s, "id"))?;
    let mut p = Map::new();
    p.insert("type".to_string(), json!("vmess"));
    p.insert("server".to_string(), json!(server));
    p.insert("port".to_string(), json!(port));
    p.insert("id".to_string(), json!(id));
    if let Some(cipher) = string_field(s, "cipher") {
        p.insert("cipher".to_string(), json!(cipher));
    } else if s.get("alterId").or_else(|| s.get("alterid")).is_some() {
        // Clash vmess carries alterId; Zero is AEAD-only and normalizes
        // `cipher: auto` to its AEAD baseline.
        p.insert("cipher".to_string(), json!("auto"));
    }
    if let Some(tls) = build_tls(s) {
        p.insert("tls".to_string(), Value::Object(tls));
    }
    if let Some(ws) = build_ws(s) {
        p.insert("ws".to_string(), Value::Object(ws));
    }
    if let Some(grpc) = build_grpc(s) {
        p.insert("grpc".to_string(), Value::Object(grpc));
    }
    Some(p)
}

fn build_vless(s: &Map<String, Value>) -> Option<Map<String, Value>> {
    let (server, port) = server_port(s)?;
    let id = string_field(s, "uuid").or_else(|| string_field(s, "id"))?;
    let mut p = Map::new();
    p.insert("type".to_string(), json!("vless"));
    p.insert("server".to_string(), json!(server));
    p.insert("port".to_string(), json!(port));
    p.insert("id".to_string(), json!(id));
    if let Some(reality) = build_reality(s) {
        p.insert("reality".to_string(), Value::Object(reality));
    }
    if let Some(tls) = build_tls(s) {
        p.insert("tls".to_string(), Value::Object(tls));
    }
    if let Some(ws) = build_ws(s) {
        p.insert("ws".to_string(), Value::Object(ws));
    }
    Some(p)
}

fn build_socks(s: &Map<String, Value>) -> Option<Map<String, Value>> {
    let (server, port) = server_port(s)?;
    let mut p = Map::new();
    p.insert("type".to_string(), json!("socks5"));
    p.insert("server".to_string(), json!(server));
    p.insert("port".to_string(), json!(port));
    if let Some(username) = string_field(s, "username") {
        p.insert("username".to_string(), json!(username));
    }
    if let Some(password) = string_field(s, "password") {
        p.insert("password".to_string(), json!(password));
    }
    Some(p)
}

fn build_http(s: &Map<String, Value>) -> Option<Map<String, Value>> {
    let (server, port) = server_port(s)?;
    let mut p = Map::new();
    p.insert("type".to_string(), json!("http"));
    p.insert("server".to_string(), json!(server));
    p.insert("port".to_string(), json!(port));
    if let Some(username) = string_field(s, "username") {
        p.insert("username".to_string(), json!(username));
    }
    if let Some(password) = string_field(s, "password") {
        p.insert("password".to_string(), json!(password));
    }
    if s.get("tls").and_then(Value::as_bool) == Some(true) {
        p.insert("tls".to_string(), json!(true));
    }
    Some(p)
}

fn build_hysteria2(s: &Map<String, Value>) -> Option<Map<String, Value>> {
    let (server, port) = server_port(s)?;
    let password = string_field(s, "password")?;
    let raw_type = string_field(s, "type").unwrap_or_default();
    let type_tag = if raw_type == "hysteria" {
        "hysteria"
    } else {
        "hysteria2"
    };
    let mut p = Map::new();
    p.insert("type".to_string(), json!(type_tag));
    p.insert("server".to_string(), json!(server));
    p.insert("port".to_string(), json!(port));
    p.insert("password".to_string(), json!(password));
    if let Some(insecure) = resolve_insecure(s) {
        p.insert("insecure".to_string(), json!(insecure));
    }
    if let Some(fp) = resolve_client_fingerprint(s) {
        p.insert("client_fingerprint".to_string(), json!(fp));
    }
    Some(p)
}

// ── TLS / transport builders ──
//
// Clash spreads TLS across flat fields (sni, skip-cert-verify, alpn,
// disable-sni) and models transports as `ws-opts` / `grpc-opts` /
// `reality-opts`. Zero wants one nested object per outbound. Each builder
// returns None when no relevant fields are present so the caller can skip
// emitting an empty object.

fn build_tls(s: &Map<String, Value>) -> Option<Map<String, Value>> {
    let server_name = resolve_sni(s);
    let insecure = resolve_insecure(s);
    let alpn = s.get("alpn").and_then(Value::as_array).cloned();
    let disable_sni = s
        .get("disable-sni")
        .or_else(|| s.get("disable_sni"))
        .and_then(Value::as_bool);
    if server_name.is_none() && insecure.is_none() && alpn.is_none() && disable_sni.is_none() {
        return None;
    }
    let mut tls = Map::new();
    if let Some(server_name) = server_name {
        tls.insert("server_name".to_string(), json!(server_name));
    }
    if let Some(insecure) = insecure {
        tls.insert("insecure".to_string(), json!(insecure));
    }
    if let Some(disable_sni) = disable_sni {
        tls.insert("disable_sni".to_string(), json!(disable_sni));
    }
    if let Some(alpn) = alpn {
        tls.insert("alpn".to_string(), Value::Array(alpn));
    }
    Some(tls)
}

fn build_ws(s: &Map<String, Value>) -> Option<Map<String, Value>> {
    let ws = s
        .get("ws-opts")
        .or_else(|| s.get("ws_opts"))
        .and_then(Value::as_object)?;
    let mut w = Map::new();
    if let Some(path) = string_field(ws, "path") {
        w.insert("path".to_string(), json!(path));
    }
    if let Some(headers) = ws.get("headers").and_then(Value::as_object).cloned() {
        w.insert("headers".to_string(), Value::Object(headers));
    }
    if w.is_empty() {
        return None;
    }
    Some(w)
}

fn build_grpc(s: &Map<String, Value>) -> Option<Map<String, Value>> {
    let grpc = s
        .get("grpc-opts")
        .or_else(|| s.get("grpc_opts"))
        .and_then(Value::as_object)?;
    let service = string_field(grpc, "grpc-service-name")
        .or_else(|| string_field(grpc, "grpc_service_name"))
        .or_else(|| string_field(grpc, "service-name"))
        .or_else(|| string_field(grpc, "serviceName"));
    let mut g = Map::new();
    if let Some(service) = service {
        g.insert("service_name".to_string(), json!(service));
    }
    if g.is_empty() {
        return None;
    }
    Some(g)
}

fn build_reality(s: &Map<String, Value>) -> Option<Map<String, Value>> {
    let r = s
        .get("reality-opts")
        .or_else(|| s.get("reality_opts"))
        .and_then(Value::as_object)?;
    let public_key = string_field(r, "public-key").or_else(|| string_field(r, "public_key"))?;
    let mut reality = Map::new();
    reality.insert("public_key".to_string(), json!(public_key));
    if let Some(short_id) = string_field(r, "short-id").or_else(|| string_field(r, "short_id")) {
        reality.insert("short_id".to_string(), json!(short_id));
    }
    let server_name = string_field(r, "server-name")
        .or_else(|| string_field(r, "server_name"))
        .or_else(|| resolve_sni(s));
    if let Some(server_name) = server_name {
        reality.insert("server_name".to_string(), json!(server_name));
    }
    Some(reality)
}

fn resolve_sni(s: &Map<String, Value>) -> Option<String> {
    string_field(s, "sni")
        .or_else(|| string_field(s, "servername"))
        .or_else(|| string_field(s, "server-name"))
}

fn resolve_insecure(s: &Map<String, Value>) -> Option<bool> {
    s.get("skip-cert-verify")
        .or_else(|| s.get("skip_cert_verify"))
        .or_else(|| s.get("insecure"))
        .and_then(Value::as_bool)
}

fn resolve_client_fingerprint(s: &Map<String, Value>) -> Option<String> {
    string_field(s, "client-fingerprint")
        .or_else(|| string_field(s, "client_fingerprint"))
        .or_else(|| string_field(s, "fingerprint"))
}

fn convert_clash_proxy_group(
    group: &Value,
    known_tags: &std::collections::BTreeSet<String>,
) -> Option<Value> {
    let source = group.as_object()?;
    let tag = string_field(source, "name")?;
    let group_type = string_field(source, "type")
        .unwrap_or_else(|| "select".to_string())
        .to_ascii_lowercase();

    let members: Vec<Value> = source
        .get("proxies")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|t| normalize_outbound_ref(t, known_tags))
        .map(Value::String)
        .collect();
    if members.is_empty() {
        return None;
    }

    let mapped_type = match group_type.as_str() {
        "url-test" => "url_test",
        "fallback" => "fallback",
        "load-balance" => "load_balance",
        "relay" => "relay",
        _ => "selector",
    };

    // Zero's `relay` group carries its chain under `proxies`; every other
    // group type uses `outbounds`. Both are populated from clash's `proxies`.
    let members_key = if mapped_type == "relay" {
        "proxies"
    } else {
        "outbounds"
    };

    let mut converted = Map::new();
    converted.insert("tag".to_string(), Value::String(tag));
    converted.insert("type".to_string(), Value::String(mapped_type.to_string()));
    converted.insert(members_key.to_string(), Value::Array(members));

    match mapped_type {
        "url_test" => {
            if let Some(url) = source.get("url") {
                converted.insert("url".to_string(), url.clone());
            }
            if let Some(interval) = source.get("interval") {
                converted.insert("interval_seconds".to_string(), interval.clone());
            }
        }
        "load_balance" => {
            if let Some(strategy) = string_field(source, "strategy") {
                converted.insert("strategy".to_string(), Value::String(strategy));
            }
        }
        _ => {}
    }

    Some(Value::Object(converted))
}

fn convert_clash_rule(
    rule: &Value,
    known_tags: &std::collections::BTreeSet<String>,
) -> Option<Value> {
    let raw = rule.as_str()?.trim();
    let parts = raw.split(',').map(str::trim).collect::<Vec<_>>();
    if parts.len() < 2 {
        return None;
    }

    let rule_type = parts[0].to_ascii_uppercase();
    if rule_type == "MATCH" {
        return None;
    }

    let outbound = normalize_outbound_ref(parts.last()?, known_tags)?;

    let value = parts.get(1)?.to_string();
    let condition = match rule_type.as_str() {
        "DOMAIN" => json!({ "type": "domain_regex", "values": [exact_domain_regex(&value)] }),
        "DOMAIN-SUFFIX" => json!({ "type": "domain", "values": [value] }),
        "DOMAIN-KEYWORD" => json!({ "type": "domain_keyword", "values": [value] }),
        "IP-CIDR" | "IP-CIDR6" => json!({ "type": "ip", "values": [value] }),
        "GEOIP" => json!({ "type": "geoip", "values": [value] }),
        "RULE-SET" => json!({ "type": "rule_set", "tag": value }),
        _ => return None,
    };

    Some(json!({
        "condition": condition,
        "action": { "type": "route", "outbound": outbound }
    }))
}

fn exact_domain_regex(domain: &str) -> String {
    let mut escaped = String::with_capacity(domain.len() + 8);
    escaped.push_str("(?i)^");
    for character in domain.chars() {
        if matches!(
            character,
            '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' | '\\'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped.push('$');
    escaped
}

fn clash_match_outbound(
    rule: &Value,
    known_tags: &std::collections::BTreeSet<String>,
) -> Option<String> {
    let raw = rule.as_str()?.trim();
    let parts = raw.split(',').map(str::trim).collect::<Vec<_>>();
    if parts
        .first()
        .is_some_and(|part| part.eq_ignore_ascii_case("MATCH"))
    {
        return normalize_outbound_ref(parts.last()?, known_tags);
    }
    None
}

fn normalize_outbound_ref(
    tag: &str,
    known_tags: &std::collections::BTreeSet<String>,
) -> Option<String> {
    let trimmed = tag.trim();
    if known_tags.contains(trimmed) {
        return Some(trimmed.to_string());
    }

    match trimmed.to_ascii_uppercase().as_str() {
        "DIRECT" | "PASS" => Some("direct".to_string()),
        "REJECT" => Some("block".to_string()),
        other => {
            crate::services::file_logger::emit(
                "warn",
                "subscription",
                &format!("drop unknown outbound ref: {other}"),
                None,
            );
            None
        }
    }
}

fn string_field(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

// Per-protocol outbound construction lives in the `build_*` functions above.
// Zero's strict serde needs an explicit field set per protocol, so a generic
// key map is not enough — each protocol builds its own object.

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
            message: format!("subscription response must be base64 encoded: {error}"),
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

async fn upsert_synced_proxy_config(
    app_handle: &AppHandle,
    state: &AppState,
    subscription: &SubscriptionProfile,
    target_proxy_config_id: &str,
    mut parsed: ParsedSubscriptionConfig,
) -> AppResult<ProxyConfigProfile> {
    let existing_profile = lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.id == target_proxy_config_id)
        .cloned();
    let existing_active = existing_profile
        .as_ref()
        .is_some_and(|profile| profile.active);
    if parsed.format.contains("clash") {
        let (host, port) = {
            let config = lock(state.app_config(), "app_config")?;
            (config.local_proxy.host.clone(), config.local_proxy.port)
        };
        ensure_clash_local_inbound(
            &mut parsed.content,
            existing_profile
                .as_ref()
                .and_then(|profile| profile.content.as_ref()),
            &host,
            port,
        )?;
    }
    // A sync replaces the generated proxy config. Reapply choices that are
    // still valid before hot-loading it; stale choices are omitted, which
    // makes Zero fall back to the selector's first member.
    crate::services::policy_selection::apply_selections(
        &mut parsed.content,
        &subscription.policy_selections,
    );
    ensure_subscription_unchanged(state, subscription)?;
    let input = build_synced_proxy_config_upsert(
        subscription,
        target_proxy_config_id,
        parsed.content,
        existing_active,
    );
    proxy_config::upsert_runtime(app_handle.clone(), input).await
}

fn build_synced_proxy_config_upsert(
    subscription: &SubscriptionProfile,
    target_proxy_config_id: &str,
    content: Value,
    active: bool,
) -> ProxyConfigUpsert {
    ProxyConfigUpsert {
        id: Some(target_proxy_config_id.to_string()),
        name: subscription.name.clone(),
        kernel: Some(subscription.kernel.clone()),
        // `ParsedSubscriptionConfig::format` describes how the source was
        // decoded or converted. At this point the payload is already a JSON
        // value, and proxy profiles deliberately accept only JSON.
        format: Some("json".to_string()),
        path: Some(subscription.url.clone()),
        content: Some(content),
        active: Some(active),
    }
}

fn ensure_clash_local_inbound(
    content: &mut Value,
    existing_content: Option<&Value>,
    host: &str,
    port: u16,
) -> AppResult<()> {
    let object = content.as_object_mut().ok_or_else(|| {
        AppError::invalid_argument("converted Clash subscription must produce an object")
    })?;
    if object
        .get("inbounds")
        .and_then(Value::as_array)
        .is_some_and(|inbounds| !inbounds.is_empty())
    {
        return Ok(());
    }

    let inbounds = existing_content
        .and_then(|existing| existing.get("inbounds"))
        .and_then(Value::as_array)
        .filter(|inbounds| {
            inbounds.iter().any(|inbound| {
                crate::services::proxy_config::extract_local_proxy(&json!({
                    "inbounds": [inbound]
                }))
                .is_some()
            })
        })
        .cloned()
        .unwrap_or_else(|| {
            vec![json!({
                "tag": "mixed-in",
                "listen": { "address": host, "port": port },
                "protocol": { "type": "mixed" }
            })]
        });
    object.insert("inbounds".to_string(), Value::Array(inbounds));
    Ok(())
}

fn update_sync_success(
    state: &AppState,
    id: &str,
    target_proxy_config_id: String,
    metadata: SyncMetadata,
    synced_at_unix_ms: u64,
) -> AppResult<SubscriptionProfile> {
    let synced_content = lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.id == target_proxy_config_id)
        .and_then(|profile| profile.content.clone());
    let mut subscriptions = lock(state.subscriptions(), "subscription")?;
    let mut next = subscriptions.clone();
    let subscription = next
        .iter_mut()
        .find(|profile| profile.id == id)
        .ok_or_else(|| AppError::not_found("subscription", id.to_string()))?;

    subscription.target_proxy_config_id = Some(target_proxy_config_id);
    if let Some(content) = synced_content.as_ref() {
        crate::services::policy_selection::retain_valid_selections(
            &mut subscription.policy_selections,
            content,
        );
    }
    subscription.last_sync_at_unix_ms = Some(synced_at_unix_ms);
    subscription.last_error = None;
    subscription.updated_at_unix_ms = synced_at_unix_ms;
    subscription.node_count = metadata.node_count;
    subscription.upload_bytes = metadata.upload_bytes;
    subscription.download_bytes = metadata.download_bytes;
    subscription.total_bytes = metadata.total_bytes;
    subscription.expire_at_unix_ms = metadata.expire_at_unix_ms;
    let updated = subscription.clone();
    domain_store::save_subscriptions(&next)?;
    *subscriptions = next;

    Ok(updated)
}

fn update_sync_error(state: &AppState, id: &str, message: &str) -> AppResult<()> {
    let mut subscriptions = lock(state.subscriptions(), "subscription")?;
    let mut next = subscriptions.clone();
    if let Some(subscription) = next.iter_mut().find(|profile| profile.id == id) {
        subscription.last_error = Some(message.to_string());
        subscription.updated_at_unix_ms = now_unix_ms();
        domain_store::save_subscriptions(&next)?;
        *subscriptions = next;
    }

    Ok(())
}

// ── Auto-sync scheduler ──

/// Spawn the background auto-sync loop. Runs for the lifetime of the
/// app: every [`AUTO_SYNC_TICK_SECONDS`] it re-syncs any subscription
/// that is `enabled`, has an `update_interval_secs`, and is overdue.
pub fn spawn_auto_sync_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Warmup: let the kernel / network come up before the first pass.
        tokio::time::sleep(Duration::from_secs(AUTO_SYNC_WARMUP_SECONDS)).await;
        let mut retry_states = HashMap::new();

        loop {
            run_auto_sync_pass(&app, &mut retry_states).await;
            tokio::time::sleep(Duration::from_secs(AUTO_SYNC_TICK_SECONDS)).await;
        }
    });
}

async fn run_auto_sync_pass(
    app: &AppHandle,
    retry_states: &mut HashMap<String, AutoSyncRetryState>,
) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };

    let now = now_unix_ms();
    let due = match collect_due_subscription_attempts(state.inner(), retry_states, now) {
        Ok(attempts) => attempts,
        Err(error) => {
            logs::znet_log_fields(
                Some(state.inner()),
                LogLevel::Warn,
                format!(
                    "subscription auto-sync: failed to collect due subscriptions: {}",
                    error.message
                ),
                json!({
                    "schema": "znet.subscription-sync.v1",
                    "operation": "collect_due",
                    "errorCode": error.code,
                    "errorMessage": error.message,
                }),
            );
            return;
        }
    };

    if due.is_empty() {
        return;
    }

    for attempt in due {
        let attempt_label = match &attempt.kind {
            AutoSyncAttemptKind::Initial => "scheduled attempt".to_string(),
            AutoSyncAttemptKind::Retry { number } => {
                format!("retry {number}/{AUTO_SYNC_MAX_RETRIES}")
            }
        };
        match sync_by_id(app, state.inner(), &attempt.id).await {
            Ok(profile) => {
                retry_states.remove(&attempt.id);
                logs::znet_log_fields(
                    Some(state.inner()),
                    LogLevel::Info,
                    format!(
                        "subscription auto-sync: refreshed '{}' ({} nodes)",
                        profile.name,
                        profile.node_count.unwrap_or(0)
                    ),
                    json!({
                        "schema": "znet.subscription-sync.v1",
                        "operation": "auto_sync",
                        "subscriptionId": attempt.id,
                        "subscriptionName": profile.name,
                        "attempt": attempt_label,
                        "outcome": "success",
                        "nodeCount": profile.node_count.unwrap_or(0),
                    }),
                );
            }
            Err(error) if error.code == "conflict" => {
                logs::znet_log_fields(
                    Some(state.inner()),
                    LogLevel::Debug,
                    format!(
                        "subscription auto-sync: skipped '{}' because another sync is already running",
                        attempt.name
                    ),
                    json!({
                        "schema": "znet.subscription-sync.v1",
                        "operation": "auto_sync",
                        "subscriptionId": attempt.id,
                        "subscriptionName": attempt.name,
                        "attempt": attempt_label,
                        "outcome": "skipped_conflict",
                    }),
                );
            }
            Err(error) => {
                let disposition = record_auto_sync_failure(retry_states, &attempt, now_unix_ms());
                let (message, retry_number, delay_secs, retry_exhausted) = match disposition {
                    AutoSyncFailureDisposition::RetryScheduled {
                        retry_number,
                        delay_secs,
                        ..
                    } => (
                        format!(
                            "subscription auto-sync: '{}' {attempt_label} failed: {}; retry {retry_number}/{AUTO_SYNC_MAX_RETRIES} in {delay_secs}s",
                            attempt.name, error.message,
                        ),
                        retry_number,
                        delay_secs,
                        false,
                    ),
                    AutoSyncFailureDisposition::RetryLimitReached { cooldown_secs, .. } => (
                        format!(
                            "subscription auto-sync: '{}' {attempt_label} failed after {AUTO_SYNC_MAX_RETRIES} retries: {}; automatic retries paused for {cooldown_secs}s",
                            attempt.name, error.message,
                        ),
                        AUTO_SYNC_MAX_RETRIES,
                        cooldown_secs,
                        true,
                    ),
                };
                logs::znet_log_fields(
                    Some(state.inner()),
                    LogLevel::Warn,
                    message,
                    json!({
                        "schema": "znet.subscription-sync.v1",
                        "operation": "auto_sync",
                        "subscriptionId": attempt.id,
                        "subscriptionName": attempt.name,
                        "attempt": attempt_label,
                        "outcome": "failure",
                        "errorCode": error.code,
                        "errorMessage": error.message,
                        "retryNumber": retry_number,
                        "retryLimit": AUTO_SYNC_MAX_RETRIES,
                        "nextDelaySecs": delay_secs,
                        "retryExhausted": retry_exhausted,
                    }),
                );
            }
        }
    }
}

/// Identify subscriptions that are enabled, have an auto-sync
/// interval, and whose last sync predates the interval window.
#[cfg(test)]
pub(crate) fn collect_due_subscription_ids(state: &AppState) -> AppResult<Vec<String>> {
    let mut retry_states = HashMap::new();
    Ok(
        collect_due_subscription_attempts(state, &mut retry_states, now_unix_ms())?
            .into_iter()
            .map(|attempt| attempt.id)
            .collect(),
    )
}

fn collect_due_subscription_attempts(
    state: &AppState,
    retry_states: &mut HashMap<String, AutoSyncRetryState>,
    now_unix_ms: u64,
) -> AppResult<Vec<DueSubscription>> {
    let subscriptions = lock(state.subscriptions(), "subscription")?;

    // Drop retry state when a subscription is removed, disabled, switched to
    // manual mode, or successfully synced outside this scheduler.
    retry_states.retain(|id, retry| {
        subscriptions.iter().any(|profile| {
            profile.id == *id
                && profile.enabled
                && profile.update_interval_secs.is_some()
                && profile.last_sync_at_unix_ms == retry.cycle_last_sync_at_unix_ms
        })
    });

    let mut reset_cycles = Vec::new();
    let mut due = Vec::new();

    for profile in subscriptions.iter() {
        if !profile.enabled {
            continue;
        }
        if is_in_flight(state.subscription_syncs(), &profile.id)? {
            continue;
        }
        let Some(interval_secs) = profile.update_interval_secs else {
            continue;
        };

        let kind = if let Some(retry) = retry_states.get(&profile.id) {
            if now_unix_ms < retry.next_attempt_at_unix_ms {
                continue;
            }

            if retry.failed_attempts > AUTO_SYNC_MAX_RETRIES {
                // The cooldown after an exhausted cycle has elapsed. Start a
                // fresh bounded cycle instead of carrying the old budget on.
                reset_cycles.push(profile.id.clone());
                AutoSyncAttemptKind::Initial
            } else {
                AutoSyncAttemptKind::Retry {
                    number: retry.failed_attempts,
                }
            }
        } else {
            let interval_ms = interval_secs.saturating_mul(1_000);
            let due_at = profile
                .last_sync_at_unix_ms
                .unwrap_or(0)
                .saturating_add(interval_ms);
            if now_unix_ms < due_at {
                continue;
            }
            AutoSyncAttemptKind::Initial
        };

        due.push(DueSubscription {
            id: profile.id.clone(),
            name: profile.name.clone(),
            interval_secs,
            last_sync_at_unix_ms: profile.last_sync_at_unix_ms,
            kind,
        });
    }

    for id in reset_cycles {
        retry_states.remove(&id);
    }

    Ok(due)
}

fn auto_sync_retry_delay_secs(retry_number: u32) -> u64 {
    let exponent = retry_number.saturating_sub(1).min(31);
    AUTO_SYNC_RETRY_BASE_SECONDS
        .saturating_mul(1_u64 << exponent)
        .min(AUTO_SYNC_RETRY_MAX_SECONDS)
}

fn record_auto_sync_failure(
    retry_states: &mut HashMap<String, AutoSyncRetryState>,
    attempt: &DueSubscription,
    failed_at_unix_ms: u64,
) -> AutoSyncFailureDisposition {
    let failed_attempts = retry_states
        .get(&attempt.id)
        .map(|retry| retry.failed_attempts)
        .unwrap_or(0)
        .saturating_add(1);

    if failed_attempts <= AUTO_SYNC_MAX_RETRIES {
        let delay_secs = auto_sync_retry_delay_secs(failed_attempts);
        let next_attempt_at_unix_ms =
            failed_at_unix_ms.saturating_add(delay_secs.saturating_mul(1_000));
        retry_states.insert(
            attempt.id.clone(),
            AutoSyncRetryState {
                failed_attempts,
                next_attempt_at_unix_ms,
                cycle_last_sync_at_unix_ms: attempt.last_sync_at_unix_ms,
            },
        );
        AutoSyncFailureDisposition::RetryScheduled {
            retry_number: failed_attempts,
            delay_secs,
            next_attempt_at_unix_ms,
        }
    } else {
        // Once the retry budget is exhausted, wait for one normal update
        // interval before allowing a fresh bounded cycle.
        let cooldown_secs = attempt.interval_secs.max(AUTO_SYNC_TICK_SECONDS);
        let next_cycle_at_unix_ms =
            failed_at_unix_ms.saturating_add(cooldown_secs.saturating_mul(1_000));
        retry_states.insert(
            attempt.id.clone(),
            AutoSyncRetryState {
                failed_attempts,
                next_attempt_at_unix_ms: next_cycle_at_unix_ms,
                cycle_last_sync_at_unix_ms: attempt.last_sync_at_unix_ms,
            },
        );
        AutoSyncFailureDisposition::RetryLimitReached {
            cooldown_secs,
            next_cycle_at_unix_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clash_rule_providers_are_kept_for_zrs_synchronization() {
        let parsed = parse_subscription_content(
            r#"
proxies:
  - {name: HK, type: ss, server: s, port: 1, password: p}
proxy-groups:
  - {name: Proxy, type: select, proxies: [HK]}
rules:
  - RULE-SET,Ads,REJECT
  - MATCH,Proxy
rule-providers:
  Ads: {type: http, behavior: classical, url: 'https://example.com/ads.yaml', path: ./rules/ads, interval: 3600}
"#,
            "clash",
        )
        .unwrap();

        assert_eq!(parsed.rule_providers.len(), 1);
        assert_eq!(parsed.rule_providers[0].tag, "Ads");
        assert_eq!(parsed.rule_providers[0].update_interval_secs, Some(3600));
        assert_eq!(parsed.content["route"]["rule_sets"], json!([]));
        assert_eq!(
            parsed.content["route"]["rules"][0]["condition"],
            json!({ "type": "rule_set", "tag": "Ads" })
        );
    }

    #[test]
    fn clash_rule_set_without_provider_is_dropped_without_rejecting_subscription() {
        let parsed = parse_subscription_content(
            "proxies:\n  - {name: HK, type: ss, server: s, port: 1, password: p}\nrules:\n  - RULE-SET,Missing,DIRECT\n",
            "clash",
        )
        .unwrap();

        assert!(parsed.rule_providers.is_empty());
        assert!(parsed.content["route"]["rules"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn unavailable_provider_drops_only_its_route_reference() {
        let mut content = json!({
            "route": {
                "rules": [
                    {"condition":{"type":"rule_set","tag":"Missing"},"action":{"type":"reject"}},
                    {"condition":{"type":"rule_set","tag":"Available"},"action":{"type":"direct"}},
                    {"condition":{"type":"domain","values":["example.com"]},"action":{"type":"direct"}}
                ]
            }
        });
        let removed = inject_synced_rule_sets(
            &mut content,
            vec![rule_set::ManagedRuleSetArtifact {
                tag: "Available".to_string(),
                path: "available.zrs".to_string(),
            }],
        )
        .unwrap();

        assert_eq!(removed, vec!["Missing".to_string()]);
        let rules = content["route"]["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0]["condition"]["tag"], "Available");
        assert_eq!(rules[1]["condition"]["type"], "domain");
    }

    #[test]
    fn legacy_dler_raw_provider_urls_use_the_live_github_raw_origin() {
        assert_eq!(
            normalize_clash_rule_provider_url(
                "https://raw.dler.io/dler-io/Rules/main/Clash/Provider/AdBlock.yaml"
            ),
            "https://raw.githubusercontent.com/dler-io/Rules/main/Clash/Provider/AdBlock.yaml"
        );
        assert_eq!(
            normalize_clash_rule_provider_url("https://example.com/rules.yaml"),
            "https://example.com/rules.yaml"
        );
    }

    #[test]
    fn clash_and_auto_fetches_request_the_full_clash_document() {
        assert_eq!(default_user_agent_for_format("auto"), "Clash.Meta");
        assert_eq!(default_user_agent_for_format("clash-yaml"), "Clash.Meta");
        assert!(default_user_agent_for_format("zero-json").starts_with("ZNet-Sink/"));
    }

    #[test]
    fn clash_route_conditions_follow_the_zero_config_contract() {
        let parsed = parse_subscription_content(
            "proxies:\n  - {name: HK, type: ss, server: s, port: 1, password: p}\nrules:\n  - DOMAIN,exact.example,HK\n  - DOMAIN-SUFFIX,suffix.example,HK\n  - IP-CIDR,10.0.0.0/8,DIRECT\n  - MATCH,HK\n",
            "clash",
        )
        .unwrap();
        let rules = parsed.content["route"]["rules"].as_array().unwrap();

        assert_eq!(rules[0]["condition"]["type"], "domain_regex");
        assert_eq!(rules[0]["condition"]["values"][0], "(?i)^exact\\.example$");
        assert_eq!(rules[1]["condition"]["type"], "domain");
        assert_eq!(rules[2]["condition"]["type"], "ip");
    }

    #[test]
    #[ignore = "requires ZNET_REAL_SUBSCRIPTION_URL and live provider access"]
    fn live_clash_subscription_rule_providers_compile_to_verified_zrs() {
        use zero_rule::protocol::decode_json;
        use zero_rule::zrs::{encode, verify, VerifyMode};
        use zero_rule::RuleSetCompiler;

        let url = std::env::var("ZNET_REAL_SUBSCRIPTION_URL")
            .expect("ZNET_REAL_SUBSCRIPTION_URL is required");
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Clash.Meta")
            .build()
            .unwrap();
        let body = client
            .get(url)
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .text()
            .unwrap();
        let parsed = parse_subscription_content(&body, "clash").unwrap();
        let provider_count = parsed.rule_providers.len();
        let referenced_count = parsed.content["route"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|rule| rule["condition"]["type"] == "rule_set")
            .count();
        assert_eq!(provider_count, referenced_count);

        let mut total_entries = 0u64;
        for provider in parsed.rule_providers {
            let content = client
                .get(provider.url)
                .send()
                .unwrap()
                .error_for_status()
                .unwrap()
                .text()
                .unwrap();
            let ir = rule_set::convert_managed_clash_source(&content, &provider.tag).unwrap();
            let source = decode_json(&serde_json::to_vec(&ir).unwrap()).unwrap();
            let (compiled, _) = RuleSetCompiler.compile(source).unwrap();
            let bytes = encode(&compiled).unwrap();
            let metadata = verify(&bytes, VerifyMode::FullChecksum).unwrap();
            total_entries += metadata.entry_count();
        }
        assert_eq!(provider_count, 60);
        assert!(total_entries > 10_000);
    }

    #[test]
    fn clash_conversion_gets_a_usable_mixed_inbound() {
        let mut content = json!({ "outbounds": [] });
        ensure_clash_local_inbound(&mut content, None, "127.0.0.1", 7890).unwrap();

        let endpoint = proxy_config::extract_local_proxy(&content).unwrap();
        assert_eq!(endpoint.host, "127.0.0.1");
        assert_eq!(endpoint.port, 7890);
    }

    #[test]
    fn clash_conversion_preserves_existing_local_inbounds() {
        let existing = json!({
            "inbounds": [{
                "tag": "custom-mixed",
                "listen": { "address": "127.0.0.1", "port": 8899 },
                "protocol": { "type": "mixed" }
            }]
        });
        let mut content = json!({ "outbounds": [] });
        ensure_clash_local_inbound(&mut content, Some(&existing), "127.0.0.1", 7890).unwrap();

        let endpoint = proxy_config::extract_local_proxy(&content).unwrap();
        assert_eq!(endpoint.port, 8899);
    }

    #[test]
    fn synced_proxy_config_uses_json_target_format_after_source_conversion() {
        let subscription = SubscriptionProfile {
            id: "subscription-1".to_string(),
            name: "Converted Clash".to_string(),
            url: "https://example.com/subscription".to_string(),
            enabled: true,
            kernel: "zero".to_string(),
            format: "clash-yaml".to_string(),
            target_proxy_config_id: None,
            policy_selections: Default::default(),
            update_interval_secs: None,
            user_agent: None,
            node_count: None,
            upload_bytes: None,
            download_bytes: None,
            total_bytes: None,
            expire_at_unix_ms: None,
            updated_at_unix_ms: 1,
            last_sync_at_unix_ms: None,
            last_error: None,
        };
        let content = json!({ "outbounds": [] });

        let input = build_synced_proxy_config_upsert(
            &subscription,
            "proxy-config-1",
            content.clone(),
            true,
        );

        assert_eq!(input.format.as_deref(), Some("json"));
        assert_eq!(input.path.as_deref(), Some(subscription.url.as_str()));
        assert_eq!(input.content, Some(content));
        assert_eq!(input.active, Some(true));
    }

    #[test]
    fn auto_detect_accepts_raw_zero_json() {
        let parsed =
            parse_subscription_content(r#"{"outbounds":[{"tag":"hk","type":"trojan"}]}"#, "auto")
                .unwrap();
        assert_eq!(parsed.format, "zero-json");
        assert!(parsed.content.get("outbounds").is_some());
    }

    #[test]
    fn auto_detect_rejects_unrelated_json() {
        // A JSON object with none of the known Zero keys should not
        // be accepted as a config in auto mode.
        let error = parse_subscription_content(r#"{"hello":"world"}"#, "auto").unwrap_err();
        assert_eq!(error.code, "invalid_argument");
    }

    #[test]
    fn auto_detect_accepts_base64_clash_yaml() {
        // Minimal clash yaml: `proxies:\n- {name: x, type: ss}`
        let yaml = "proxies:\n  - {name: x, type: ss, server: s, port: 1, password: p}\n";
        let encoded = general_purpose::STANDARD.encode(yaml.as_bytes());
        let parsed = parse_subscription_content(&encoded, "auto").unwrap();
        assert_eq!(parsed.format, "clash-base64-yaml-converted");
        assert_eq!(parsed.content["outbounds"][2]["tag"], "x");
    }

    #[test]
    fn clash_proxy_converts_to_nested_protocol() {
        // Zero outbounds use a nested protocol object:
        //   {"tag":...,"protocol":{"type":"trojan","server":...,...}}
        // Regression guard for the flat `type` bug — the kernel rejects a
        // top-level type with "unknown field `type`, expected `tag` or `protocol`".
        let yaml = "proxies:\n  - {name: hk, type: trojan, server: example.com, port: 443, password: secret}\n";
        let encoded = general_purpose::STANDARD.encode(yaml.as_bytes());
        let parsed = parse_subscription_content(&encoded, "auto").unwrap();
        let node = &parsed.content["outbounds"][2];
        assert_eq!(node["tag"], "hk");
        assert_eq!(node["protocol"]["type"], "trojan");
        assert_eq!(node["protocol"]["server"], "example.com");
        assert_eq!(node["protocol"]["port"], 443);
        assert!(
            node.get("type").is_none(),
            "must not emit flat top-level type"
        );
    }

    #[test]
    fn clash_vmess_emits_nested_tls_and_ws() {
        // vmess with TLS + WebSocket: clash's flat sni/skip-cert-verify and
        // nested ws-opts must land in Zero's `tls` / `ws` objects.
        let yaml = "proxies:\n  - {name: vm, type: vmess, server: s, port: 443, uuid: 11111111-2222-3333-4444-555555555555, cipher: auto, sni: s.example, skip-cert-verify: true, ws-opts: {path: /v, headers: {Host: s.example}}}\n";
        let parsed = parse_subscription_content(yaml, "clash").unwrap();
        let node = &parsed.content["outbounds"][2];
        assert_eq!(node["tag"], "vm");
        assert_eq!(node["protocol"]["type"], "vmess");
        assert_eq!(
            node["protocol"]["id"],
            "11111111-2222-3333-4444-555555555555"
        );
        assert_eq!(node["protocol"]["cipher"], "auto");
        assert_eq!(node["protocol"]["tls"]["server_name"], "s.example");
        assert_eq!(node["protocol"]["tls"]["insecure"], true);
        assert_eq!(node["protocol"]["ws"]["path"], "/v");
        assert!(node.get("type").is_none());
    }

    #[test]
    fn clash_vless_emits_reality_object() {
        let yaml = "proxies:\n  - {name: vl, type: vless, server: s, port: 443, uuid: 11111111-2222-3333-4444-555555555555, reality-opts: {public-key: PUBKEY, short-id: abcd1234, server-name: www.cloudflare.com}}\n";
        let parsed = parse_subscription_content(yaml, "clash").unwrap();
        let node = &parsed.content["outbounds"][2];
        assert_eq!(node["protocol"]["type"], "vless");
        assert_eq!(node["protocol"]["reality"]["public_key"], "PUBKEY");
        assert_eq!(node["protocol"]["reality"]["short_id"], "abcd1234");
        assert_eq!(
            node["protocol"]["reality"]["server_name"],
            "www.cloudflare.com"
        );
    }

    #[test]
    fn clash_relay_group_uses_proxies_key() {
        // Zero's relay group carries its chain under `proxies`, not `outbounds`.
        let yaml = "proxies:\n  - {name: A, type: ss, server: s, port: 1, password: p}\nproxy-groups:\n  - {name: R, type: relay, proxies: [A]}\n";
        let parsed = parse_subscription_content(yaml, "clash").unwrap();
        let groups = parsed.content["outbound_groups"].as_array().unwrap();
        let relay = groups.iter().find(|g| g["tag"] == "R").unwrap();
        assert_eq!(relay["type"], "relay");
        assert!(relay.get("proxies").is_some());
        assert!(relay.get("outbounds").is_none());
    }

    #[test]
    fn userinfo_header_parsing() {
        let info = parse_subscription_userinfo(
            "upload=1000; download=2000; total=5000; expire=1700000000",
        );
        assert_eq!(info.upload, Some(1000));
        assert_eq!(info.download, Some(2000));
        assert_eq!(info.total, Some(5000));
        assert_eq!(info.expire_secs, Some(1700000000));
        assert_eq!(info.expire_ms(), Some(1700000000000));
    }

    #[test]
    fn userinfo_header_ignores_garbage() {
        let info = parse_subscription_userinfo("upload=abc; ; download=5");
        assert_eq!(info.upload, None);
        assert_eq!(info.download, Some(5));
        assert_eq!(info.expire_ms(), None);
    }

    #[test]
    fn count_proxy_nodes_excludes_special_outbounds() {
        let content = json!({
            "outbounds": [
                {"tag": "direct", "type": "direct"},
                {"tag": "block", "type": "block"},
                {"tag": "hk", "type": "trojan"},
                {"tag": "jp", "type": "shadowsocks"},
                {"tag": "auto", "type": "urltest"}
            ]
        });
        assert_eq!(count_proxy_nodes(&content), 2);
    }

    #[test]
    fn clash_conversion_preserves_nested_group_references() {
        // A `select` group ("Final") references an `url-test` group ("Auto"),
        // and a second `select` group ("Meta") references only other groups.
        // The kernel supports nesting groups inside groups, so all three
        // groups must survive with their intra-group references intact.
        let yaml = "\
proxies:
  - {name: HK, type: ss, server: s, port: 1, password: p}
  - {name: JP, type: ss, server: s, port: 2, password: p}
proxy-groups:
  - {name: Auto, type: url-test, proxies: [HK, JP], url: http://x, interval: 300}
  - {name: Final, type: select, proxies: [Auto, DIRECT]}
  - {name: Meta, type: select, proxies: [Auto, Final]}
";
        let parsed = parse_subscription_content(yaml, "clash").unwrap();
        let groups = parsed.content["outbound_groups"].as_array().unwrap();

        let auto = groups.iter().find(|g| g["tag"] == "Auto").unwrap();
        assert_eq!(auto["type"], "url_test");
        assert_eq!(auto["outbounds"].as_array().unwrap().len(), 2);
        assert_eq!(auto["interval_seconds"], 300);

        let final_group = groups.iter().find(|g| g["tag"] == "Final").unwrap();
        let final_refs: Vec<&str> = final_group["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(final_refs, vec!["Auto", "direct"]);

        // A group that references only other groups must not be dropped.
        let meta = groups.iter().find(|g| g["tag"] == "Meta").unwrap();
        let meta_refs: Vec<&str> = meta["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(meta_refs, vec!["Auto", "Final"]);
    }
    #[test]
    fn update_interval_floor_is_enforced() {
        assert!(validate_update_interval(Some(10)).is_err());
        assert_eq!(validate_update_interval(Some(0)).unwrap(), None);
        assert_eq!(validate_update_interval(Some(120)).unwrap(), Some(120));
        assert_eq!(validate_update_interval(None).unwrap(), None);
    }

    #[test]
    fn scheduler_picks_only_due_enabled_subscriptions() {
        use crate::models::app_config::AppConfig;
        use crate::state::app_state::AppState;

        let now = now_unix_ms();
        let one_hour_ago = now.saturating_sub(3_600_000);

        let mk = |id: &str, enabled: bool, interval: Option<u64>, last_sync: Option<u64>| {
            SubscriptionProfile {
                id: id.to_string(),
                name: id.to_string(),
                url: "https://example.com/sub".to_string(),
                enabled,
                kernel: "zero".to_string(),
                format: "auto".to_string(),
                target_proxy_config_id: None,
                policy_selections: Default::default(),
                update_interval_secs: interval,
                user_agent: None,
                node_count: None,
                upload_bytes: None,
                download_bytes: None,
                total_bytes: None,
                expire_at_unix_ms: None,
                updated_at_unix_ms: now,
                last_sync_at_unix_ms: last_sync,
                last_error: None,
            }
        };

        let state = AppState::new(AppConfig::default());
        {
            let mut subs = state.subscriptions().lock().unwrap();
            // due: enabled, 1h interval, last sync 1h ago
            subs.push(mk("due", true, Some(3600), Some(one_hour_ago)));
            // not due: synced recently relative to interval
            subs.push(mk("fresh", true, Some(3600), Some(now)));
            // skipped: disabled even though overdue
            subs.push(mk("disabled", false, Some(3600), Some(one_hour_ago)));
            // skipped: no interval (manual)
            subs.push(mk("manual", true, None, Some(one_hour_ago)));
        }

        let ids = collect_due_subscription_ids(&state).unwrap();
        assert_eq!(ids, vec!["due".to_string()]);
    }

    #[test]
    fn auto_sync_retry_delay_is_exponential_and_capped() {
        assert_eq!(auto_sync_retry_delay_secs(1), 60);
        assert_eq!(auto_sync_retry_delay_secs(2), 120);
        assert_eq!(auto_sync_retry_delay_secs(3), 240);
        assert_eq!(auto_sync_retry_delay_secs(8), AUTO_SYNC_RETRY_MAX_SECONDS);
    }

    #[test]
    fn auto_sync_retry_cycle_has_a_finite_budget_and_cooldown() {
        use crate::models::app_config::AppConfig;
        use crate::state::app_state::AppState;

        let interval_secs = 3_600;
        let initial_now = 10_000_000;
        let last_sync = initial_now - interval_secs * 1_000;
        let state = AppState::new(AppConfig::default());
        state
            .subscriptions()
            .lock()
            .unwrap()
            .push(SubscriptionProfile {
                id: "bounded".to_string(),
                name: "Bounded".to_string(),
                url: "https://example.com/sub".to_string(),
                enabled: true,
                kernel: "zero".to_string(),
                format: "auto".to_string(),
                target_proxy_config_id: None,
                policy_selections: Default::default(),
                update_interval_secs: Some(interval_secs),
                user_agent: None,
                node_count: None,
                upload_bytes: None,
                download_bytes: None,
                total_bytes: None,
                expire_at_unix_ms: None,
                updated_at_unix_ms: initial_now,
                last_sync_at_unix_ms: Some(last_sync),
                last_error: None,
            });

        let mut retries = HashMap::new();
        let mut now = initial_now;
        let initial = collect_due_subscription_attempts(&state, &mut retries, now)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(initial.kind, AutoSyncAttemptKind::Initial);

        for retry_number in 1..=AUTO_SYNC_MAX_RETRIES {
            let outcome = record_auto_sync_failure(&mut retries, &initial, now);
            let AutoSyncFailureDisposition::RetryScheduled {
                retry_number: scheduled_number,
                next_attempt_at_unix_ms,
                ..
            } = outcome
            else {
                panic!("retry {retry_number} should be scheduled");
            };
            assert_eq!(scheduled_number, retry_number);
            assert!(collect_due_subscription_attempts(
                &state,
                &mut retries,
                next_attempt_at_unix_ms - 1,
            )
            .unwrap()
            .is_empty());

            now = next_attempt_at_unix_ms;
            let retry = collect_due_subscription_attempts(&state, &mut retries, now)
                .unwrap()
                .pop()
                .unwrap();
            assert_eq!(
                retry.kind,
                AutoSyncAttemptKind::Retry {
                    number: retry_number,
                }
            );
        }

        let exhausted = record_auto_sync_failure(&mut retries, &initial, now);
        let AutoSyncFailureDisposition::RetryLimitReached {
            next_cycle_at_unix_ms,
            ..
        } = exhausted
        else {
            panic!("retry budget should be exhausted");
        };
        assert!(
            collect_due_subscription_attempts(&state, &mut retries, next_cycle_at_unix_ms - 1,)
                .unwrap()
                .is_empty()
        );

        let next_cycle =
            collect_due_subscription_attempts(&state, &mut retries, next_cycle_at_unix_ms)
                .unwrap()
                .pop()
                .unwrap();
        assert_eq!(next_cycle.kind, AutoSyncAttemptKind::Initial);
        assert!(!retries.contains_key("bounded"));
    }

    #[test]
    fn successful_manual_sync_invalidates_pending_auto_retry() {
        use crate::models::app_config::AppConfig;
        use crate::state::app_state::AppState;

        let state = AppState::new(AppConfig::default());
        state
            .subscriptions()
            .lock()
            .unwrap()
            .push(SubscriptionProfile {
                id: "manual-success".to_string(),
                name: "Manual success".to_string(),
                url: "https://example.com/sub".to_string(),
                enabled: true,
                kernel: "zero".to_string(),
                format: "auto".to_string(),
                target_proxy_config_id: None,
                policy_selections: Default::default(),
                update_interval_secs: Some(60),
                user_agent: None,
                node_count: None,
                upload_bytes: None,
                download_bytes: None,
                total_bytes: None,
                expire_at_unix_ms: None,
                updated_at_unix_ms: 1,
                last_sync_at_unix_ms: None,
                last_error: None,
            });

        let mut retries = HashMap::new();
        let attempt = collect_due_subscription_attempts(&state, &mut retries, 100_000)
            .unwrap()
            .pop()
            .unwrap();
        let _ = record_auto_sync_failure(&mut retries, &attempt, 100_000);
        assert!(retries.contains_key("manual-success"));

        state.subscriptions().lock().unwrap()[0].last_sync_at_unix_ms = Some(100_001);
        assert!(
            collect_due_subscription_attempts(&state, &mut retries, 100_002)
                .unwrap()
                .is_empty()
        );
        assert!(!retries.contains_key("manual-success"));
    }
}
