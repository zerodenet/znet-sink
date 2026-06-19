use std::time::Duration;
use tauri::{AppHandle, Manager, State};

use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Map, Value};

use crate::errors::{AppError, AppResult};
use crate::models::proxy_config::ProxyConfigProfile;
use crate::models::subscription::{SubscriptionProfile, SubscriptionUpsert, SyncMetadata};
use crate::services::common::{
    generated_store_id, lock, normalize_optional, normalize_required, now_unix_ms,
};
use crate::services::domain_store;
use crate::services::proxy_config;
use crate::state::app_state::AppState;

const SUBSCRIPTION_FETCH_TIMEOUT_SECONDS: u64 = 30;
/// Default auto-sync check cadence for the background scheduler.
const AUTO_SYNC_TICK_SECONDS: u64 = 60;
/// Grace delay before the first auto-sync pass so the kernel and
/// networking stack have time to come up on startup.
const AUTO_SYNC_WARMUP_SECONDS: u64 = 15;

const DEFAULT_USER_AGENT: &str = concat!("ZNet-Sink/", env!("CARGO_PKG_VERSION"));

/// How often (in seconds) an auto-sync interval may be configured at
/// minimum. Prevents accidentally hammering a provider.
const MIN_AUTO_SYNC_INTERVAL_SECS: u64 = 60;

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
    let kernel = normalize_optional(input.kernel).unwrap_or_else(|| "zero".to_string());
    let format = normalize_optional(input.format).unwrap_or_else(|| "auto".to_string());
    let update_interval_secs = validate_update_interval(input.update_interval_secs)?;
    let user_agent = normalize_optional(input.user_agent);
    let target_proxy_config_id = normalize_optional(input.target_proxy_config_id);

    let mut subscriptions = lock(state.subscriptions(), "subscription")?;
    let profile = match subscriptions.iter_mut().find(|item| item.id == id) {
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
            subscriptions.push(profile.clone());
            profile
        }
    };
    domain_store::save_subscriptions(&subscriptions)?;

    Ok(profile)
}

pub async fn sync(state: State<'_, AppState>, id: String) -> AppResult<SubscriptionProfile> {
    let id = normalize_required(id, "id")?;
    sync_by_id(state.inner(), &id).await
}

/// Sync every enabled subscription sequentially. Returns the number
/// that succeeded. Used by the UI's "sync all" action and by the
/// background auto-sync scheduler.
pub async fn sync_all(state: State<'_, AppState>) -> AppResult<SyncAllOutcome> {
    sync_all_with_state(state.inner()).await
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

async fn sync_by_id(state: &AppState, id: &str) -> AppResult<SubscriptionProfile> {
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

    let result = sync_subscription(state, subscription).await;
    if let Err(error) = &result {
        update_sync_error(state, id, &error.message)?;
    }

    result
}

async fn sync_all_with_state(state: &AppState) -> AppResult<SyncAllOutcome> {
    let ids: Vec<String> = lock(state.subscriptions(), "subscription")?
        .iter()
        .filter(|profile| profile.enabled)
        .map(|profile| profile.id.clone())
        .collect();

    let mut succeeded = 0usize;
    for id in &ids {
        if sync_by_id(state, id).await.is_ok() {
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
    state: &AppState,
    subscription: SubscriptionProfile,
) -> AppResult<SubscriptionProfile> {
    let user_agent = subscription
        .user_agent
        .clone()
        .unwrap_or_else(|| DEFAULT_USER_AGENT.to_string());
    let response = fetch_subscription_content(subscription.url.clone(), user_agent).await?;
    let parsed = parse_subscription_content(&response.content, &subscription.format)?;
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

    upsert_synced_proxy_config(state, &subscription, &target_proxy_config_id, parsed, now)?;
    update_sync_success(
        state,
        &subscription.id,
        target_proxy_config_id,
        metadata,
        now,
    )
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
    })
}

fn parse_clash_yaml_subscription_content(content: &str) -> AppResult<ParsedSubscriptionConfig> {
    let clash: Value = serde_yaml::from_str(content).map_err(|error| AppError {
        code: "invalid_argument",
        message: format!("subscription Clash YAML is invalid: {error}"),
        details: None,
    })?;

    let content = convert_clash_to_zero(&clash)?;
    Ok(ParsedSubscriptionConfig {
        content,
        format: "clash-yaml-converted".to_string(),
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

    let content = convert_clash_to_zero(&clash)?;
    Ok(ParsedSubscriptionConfig {
        content,
        format: "clash-base64-yaml-converted".to_string(),
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

fn convert_clash_to_zero(clash: &Value) -> AppResult<Value> {
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
    outbounds.push(json!({ "tag": "direct", "type": "direct" }));
    outbounds.push(json!({ "tag": "block", "type": "block" }));
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

    let rules = root
        .get("rules")
        .and_then(Value::as_array)
        .map(|rules| {
            rules
                .iter()
                .filter_map(|rule| convert_clash_rule(rule, &known_tags))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

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

    Ok(json!({
        "outbounds": outbounds,
        "outbound_groups": outbound_groups,
        "route": {
            "rules": rules,
            "final": { "type": "route", "outbound": final_outbound }
        }
    }))
}

fn convert_clash_proxy(proxy: &Value) -> Option<Value> {
    let source = proxy.as_object()?;
    let tag = string_field(source, "name")?;
    let proxy_type = string_field(source, "type")?.to_ascii_lowercase();

    let mapped_type = match proxy_type.as_str() {
        "ss" | "shadowsocks" => "shadowsocks",
        "ssr" => "shadowsocksr",
        "vmess" => "vmess",
        "vless" => "vless",
        "trojan" => "trojan",
        "socks5" | "socks" => "socks",
        "http" | "https" => "http",
        "hysteria" | "hysteria2" | "tuic" | "wireguard" => proxy_type.as_str(),
        _ => proxy_type.as_str(),
    };

    let mut outbound = Map::new();
    outbound.insert("tag".to_string(), Value::String(tag));
    outbound.insert("type".to_string(), Value::String(mapped_type.to_string()));

    for (key, value) in source {
        if key == "name" || key == "type" {
            continue;
        }
        outbound.insert(normalize_clash_key(key), value.clone());
    }

    Some(Value::Object(outbound))
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
    let outbounds = source
        .get("proxies")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|tag| normalize_outbound_ref(tag, known_tags))
        .map(Value::String)
        .collect::<Vec<_>>();

    if outbounds.is_empty() {
        return None;
    }

    let mapped_type = match group_type.as_str() {
        "url-test" => "urltest",
        "fallback" => "fallback",
        "load-balance" => "loadbalance",
        _ => "selector",
    };

    let mut converted = Map::new();
    converted.insert("tag".to_string(), Value::String(tag));
    converted.insert("type".to_string(), Value::String(mapped_type.to_string()));
    converted.insert("outbounds".to_string(), Value::Array(outbounds));

    if let Some(url) = source.get("url") {
        converted.insert("url".to_string(), url.clone());
    }
    if let Some(interval) = source.get("interval") {
        converted.insert("interval".to_string(), interval.clone());
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
        "DOMAIN" => json!({ "type": "domain", "values": [value] }),
        "DOMAIN-SUFFIX" => json!({ "type": "domain_suffix", "values": [value] }),
        "DOMAIN-KEYWORD" => json!({ "type": "domain_keyword", "values": [value] }),
        "IP-CIDR" | "IP-CIDR6" => json!({ "type": "ip_cidr", "values": [value] }),
        "SRC-IP-CIDR" => json!({ "type": "source_ip_cidr", "values": [value] }),
        "GEOIP" => json!({ "type": "geoip", "values": [value] }),
        "GEOSITE" => json!({ "type": "geosite", "values": [value] }),
        "RULE-SET" => json!({ "type": "rule_set", "tag": value }),
        _ => return None,
    };

    Some(json!({
        "condition": condition,
        "action": { "type": "route", "outbound": outbound }
    }))
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
    if known_tags.contains(tag) {
        return Some(tag.to_string());
    }

    match tag.to_ascii_uppercase().as_str() {
        "DIRECT" => Some("direct".to_string()),
        "REJECT" => Some("block".to_string()),
        _ => None,
    }
}

fn string_field(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn normalize_clash_key(key: &str) -> String {
    key.replace('-', "_")
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
    metadata: SyncMetadata,
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
    subscription.node_count = metadata.node_count;
    subscription.upload_bytes = metadata.upload_bytes;
    subscription.download_bytes = metadata.download_bytes;
    subscription.total_bytes = metadata.total_bytes;
    subscription.expire_at_unix_ms = metadata.expire_at_unix_ms;
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

// ── Auto-sync scheduler ──

/// Spawn the background auto-sync loop. Runs for the lifetime of the
/// app: every [`AUTO_SYNC_TICK_SECONDS`] it re-syncs any subscription
/// that is `enabled`, has an `update_interval_secs`, and is overdue.
pub fn spawn_auto_sync_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Warmup: let the kernel / network come up before the first pass.
        tokio::time::sleep(Duration::from_secs(AUTO_SYNC_WARMUP_SECONDS)).await;

        loop {
            run_auto_sync_pass(&app).await;
            tokio::time::sleep(Duration::from_secs(AUTO_SYNC_TICK_SECONDS)).await;
        }
    });
}

async fn run_auto_sync_pass(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };

    let due: Vec<String> = match collect_due_subscription_ids(state.inner()) {
        Ok(ids) => ids,
        Err(error) => {
            eprintln!(
                "[ZNet] subscription auto-sync: failed to collect due ids: {}",
                error.message
            );
            return;
        }
    };

    if due.is_empty() {
        return;
    }

    for id in due {
        match sync_by_id(state.inner(), &id).await {
            Ok(profile) => eprintln!(
                "[ZNet] subscription auto-sync: refreshed '{}' ({} nodes)",
                profile.name,
                profile.node_count.unwrap_or(0)
            ),
            Err(error) => eprintln!(
                "[ZNet] subscription auto-sync: '{id}' failed: {}",
                error.message
            ),
        }
    }
}

/// Identify subscriptions that are enabled, have an auto-sync
/// interval, and whose last sync predates the interval window.
pub(crate) fn collect_due_subscription_ids(state: &AppState) -> AppResult<Vec<String>> {
    let now = now_unix_ms();
    let subscriptions = lock(state.subscriptions(), "subscription")?;
    Ok(subscriptions
        .iter()
        .filter_map(|profile| {
            if !profile.enabled {
                return None;
            }
            let interval = profile.update_interval_secs?;
            let interval_ms = u128::from(interval) * 1000;
            let last = u128::from(profile.last_sync_at_unix_ms.unwrap_or(0));
            if now as u128 >= last + interval_ms {
                Some(profile.id.clone())
            } else {
                None
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let yaml = "proxies:\n  - {name: x, type: ss, server: s, port: 1}\n";
        let encoded = general_purpose::STANDARD.encode(yaml.as_bytes());
        let parsed = parse_subscription_content(&encoded, "auto").unwrap();
        assert_eq!(parsed.format, "clash-base64-yaml-converted");
        assert_eq!(parsed.content["outbounds"][2]["tag"], "x");
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
  - {name: HK, type: ss, server: s, port: 1}
  - {name: JP, type: ss, server: s, port: 2}
proxy-groups:
  - {name: Auto, type: url-test, proxies: [HK, JP], url: http://x, interval: 300}
  - {name: Final, type: select, proxies: [Auto, DIRECT]}
  - {name: Meta, type: select, proxies: [Auto, Final]}
";
        let parsed = parse_subscription_content(yaml, "clash").unwrap();
        let groups = parsed.content["outbound_groups"].as_array().unwrap();

        let auto = groups.iter().find(|g| g["tag"] == "Auto").unwrap();
        assert_eq!(auto["type"], "urltest");
        assert_eq!(auto["outbounds"].as_array().unwrap().len(), 2);

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
}
