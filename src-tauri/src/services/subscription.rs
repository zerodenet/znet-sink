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
    let mut builder = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(SUBSCRIPTION_FETCH_TIMEOUT_SECONDS))
        .user_agent(user_agent)
        // Subscription downloads must stay independent of the proxy kernel,
        // which may be broken or absent exactly when the user needs a fresh
        // subscription to fix it. Drop the proxy_coordinator env vars first
        // (they may point at 127.0.0.1:7890 = the kernel), then opt back in
        // to a real, non-loopback OS system proxy if one is set independently.
        .no_proxy();
    if let Some(proxy_url) = real_system_proxy_url() {
        if let Ok(proxy) = reqwest::Proxy::all(&proxy_url) {
            builder = builder.proxy(proxy);
        }
    }
    let client = builder
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

/// The OS system proxy URL when it points at a real remote proxy.
///
/// Returns None for loopback proxies (127.0.0.1 / localhost / ::1 / 0.0.0.0)
/// so subscription downloads never route through the local kernel mixed-port,
/// even when the GUI has enabled its own system proxy pointing at the kernel.
/// This keeps subscription fetches independent of kernel health — the user
/// can re-sync a subscription to repair a broken kernel without that fetch
/// being tunneled through the kernel it is trying to fix.
fn real_system_proxy_url() -> Option<String> {
    let status = crate::services::system_proxy::status().ok()?;
    if !status.enabled || status.host.is_empty() || status.port == 0 {
        return None;
    }
    let is_loopback = status.host == "127.0.0.1"
        || status.host == "localhost"
        || status.host == "::1"
        || status.host == "[::1]"
        || status.host == "0.0.0.0";
    if is_loopback {
        return None;
    }
    Some(format!("http://{}:{}", status.host, status.port))
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
    let port = source
        .get("port")
        .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))?;
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
    if let Some(param) = string_field(s, "protocol-param").or_else(|| string_field(s, "protocol_param")) {
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
    let type_tag = if raw_type == "hysteria" { "hysteria" } else { "hysteria2" };
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
    let members_key = if mapped_type == "relay" { "proxies" } else { "outbounds" };

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
        assert!(node.get("type").is_none(), "must not emit flat top-level type");
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
        assert_eq!(node["protocol"]["id"], "11111111-2222-3333-4444-555555555555");
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
        assert_eq!(node["protocol"]["reality"]["server_name"], "www.cloudflare.com");
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
