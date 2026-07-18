use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};
use zero_rule::protocol::{decode_json, encode_json};
use zero_rule::zrs::{encode, verify, RuleSetMetadata, VerifyMode};
use zero_rule::RuleSetCompiler;

use crate::errors::{AppError, AppResult};
use crate::models::logs::LogLevel;
use crate::models::rule_set::{
    RuleSetKernelPayload, RuleSetProfile, RuleSetSource, RuleSetSourceState, RuleSetSyncAllOutcome,
    RuleSetUpsert, ZrsArtifact,
};
use crate::services::common::{
    begin_in_flight, generated_store_id, is_in_flight, lock, normalize_optional,
    normalize_required, now_unix_ms,
};
use crate::services::{data_dir, domain_store, logs};
use crate::state::app_state::AppState;

const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;
const MIN_UPDATE_INTERVAL_SECS: u64 = 60;
const AUTO_UPDATE_TICK_SECS: u64 = 60;
const AUTO_UPDATE_MAX_RETRIES: u32 = 3;
const AUTO_UPDATE_RETRY_BASE_SECS: u64 = 60;
const AUTO_UPDATE_RETRY_MAX_SECS: u64 = 15 * 60;
const DEFAULT_USER_AGENT: &str = concat!("ZNet-Sink/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug, PartialEq, Eq)]
struct AutoUpdateRetryState {
    failed_attempts: u32,
    next_attempt_at_unix_ms: u64,
    cycle_updated_at_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AutoUpdateAttemptKind {
    Initial,
    Retry { number: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DueRuleSet {
    id: String,
    name: String,
    interval_secs: u64,
    updated_at_unix_ms: u64,
    kind: AutoUpdateAttemptKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AutoUpdateFailureDisposition {
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

pub fn list(state: State<'_, AppState>) -> AppResult<Vec<RuleSetProfile>> {
    Ok(lock(state.rule_sets(), "rule_set")?.clone())
}

pub fn get(state: State<'_, AppState>, id: String) -> AppResult<RuleSetProfile> {
    let id = normalize_required(id, "id")?;
    lock(state.rule_sets(), "rule_set")?
        .iter()
        .find(|item| item.id == id)
        .cloned()
        .ok_or_else(|| AppError::not_found("rule_set", id))
}

pub async fn upsert(state: State<'_, AppState>, input: RuleSetUpsert) -> AppResult<RuleSetProfile> {
    let name = normalize_required(input.name, "name")?;
    let id = normalize_optional(input.id).unwrap_or_else(|| generated_store_id("rule-set"));
    let _in_flight = begin_in_flight(state.rule_set_updates(), "rule_set", &id)?;
    let previous = lock(state.rule_sets(), "rule_set")?
        .iter()
        .find(|item| item.id == id)
        .map(|item| {
            (
                item.source.clone(),
                item.source_state.clone(),
                item.last_sync_at_unix_ms,
            )
        });
    let source = input.source.map(normalize_source).transpose()?;
    let same_source = previous
        .as_ref()
        .and_then(|(source, _, _)| source.as_ref())
        .zip(source.as_ref())
        .is_some_and(|(old, new)| old.url == new.url);
    let (semantic_ir, source_state, did_sync) = match input.semantic_ir {
        Some(ir) => (
            canonical_ir(ir, &name)?,
            if same_source {
                previous
                    .as_ref()
                    .map(|(_, state, _)| state.clone())
                    .unwrap_or_default()
            } else {
                RuleSetSourceState::default()
            },
            false,
        ),
        None => {
            let source = source.as_ref().ok_or_else(|| {
                AppError::invalid_argument("semanticIr is required without a subscription source")
            })?;
            let FetchOutcome::Modified(resource) = fetch_source(source, None).await? else {
                return Err(AppError::internal(
                    "new rule source unexpectedly returned not modified",
                ));
            };
            (
                convert_source(&resource.content, &source.format, &name)?,
                resource.state,
                true,
            )
        }
    };
    let artifact = build_zrs_artifact(&id, &semantic_ir)?;
    let now = now_unix_ms();
    let profile = RuleSetProfile {
        id: id.clone(),
        name,
        enabled: input.enabled.unwrap_or(true),
        semantic_ir,
        source: source.clone(),
        source_state,
        artifact: Some(artifact),
        updated_at_unix_ms: now,
        last_sync_at_unix_ms: if did_sync {
            Some(now)
        } else if same_source {
            previous.and_then(|(_, _, last_sync)| last_sync)
        } else {
            None
        },
        last_error: None,
    };

    let mut items = lock(state.rule_sets(), "rule_set")?;
    let mut next = items.clone();
    match next.iter_mut().find(|item| item.id == id) {
        Some(existing) => *existing = profile.clone(),
        None => next.push(profile.clone()),
    }
    domain_store::save_rule_sets(&next)?;
    *items = next;
    Ok(profile)
}

/// Download an external rule file, adapt it into the canonical kernel semantics,
/// then build and atomically publish a new immutable ZRS generation.
pub async fn update(state: State<'_, AppState>, id: String) -> AppResult<RuleSetProfile> {
    update_by_id(state.inner(), id)
        .await
        .map(|(profile, _)| profile)
}

async fn update_by_id(state: &AppState, id: String) -> AppResult<(RuleSetProfile, bool)> {
    let id = normalize_required(id, "id")?;
    let _in_flight = begin_in_flight(state.rule_set_updates(), "rule_set", &id)?;
    let profile = lock(state.rule_sets(), "rule_set")?
        .iter()
        .find(|item| item.id == id)
        .cloned()
        .ok_or_else(|| AppError::not_found("rule_set", id.clone()))?;
    let source = profile
        .source
        .clone()
        .ok_or_else(|| AppError::invalid_argument("this rule asset has no subscription source"))?;
    let result = async {
        match fetch_source(&source, Some(&profile.source_state)).await? {
            FetchOutcome::NotModified(source_state) => Ok::<_, AppError>((None, source_state)),
            FetchOutcome::Modified(resource) => {
                let semantic_ir = convert_source(&resource.content, &source.format, &profile.name)?;
                let artifact = build_zrs_artifact(&profile.id, &semantic_ir)?;
                Ok((Some((semantic_ir, artifact)), resource.state))
            }
        }
    }
    .await;

    let mut items = lock(state.rule_sets(), "rule_set")?;
    let mut next = items.clone();
    let item = next
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::not_found("rule_set", id))?;
    match result {
        Ok((changed, source_state)) => {
            let was_updated = changed.is_some();
            if let Some((semantic_ir, artifact)) = changed {
                item.semantic_ir = semantic_ir;
                item.artifact = Some(artifact);
                item.last_sync_at_unix_ms = Some(now_unix_ms());
                item.updated_at_unix_ms = now_unix_ms();
            }
            item.source_state = source_state;
            item.last_error = None;
            let updated = item.clone();
            domain_store::save_rule_sets(&next)?;
            *items = next;
            Ok((updated, was_updated))
        }
        Err(error) => {
            // Keep the last known-good semantic rules and ZRS generation active.
            item.last_error = Some(error.message.clone());
            item.source_state.last_checked_at_unix_ms = Some(now_unix_ms());
            domain_store::save_rule_sets(&next)?;
            *items = next;
            Err(error)
        }
    }
}

pub async fn update_all(state: State<'_, AppState>) -> AppResult<RuleSetSyncAllOutcome> {
    update_all_with_state(state.inner()).await
}

async fn update_all_with_state(state: &AppState) -> AppResult<RuleSetSyncAllOutcome> {
    let ids = lock(state.rule_sets(), "rule_set")?
        .iter()
        .filter(|item| item.enabled && item.source.is_some())
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let mut outcome = RuleSetSyncAllOutcome {
        total: ids.len(),
        updated: 0,
        unchanged: 0,
        failed: 0,
    };
    for id in ids {
        match update_by_id(state, id).await {
            Ok((_, true)) => outcome.updated += 1,
            Ok((_, false)) => outcome.unchanged += 1,
            Err(_) => outcome.failed += 1,
        }
    }
    Ok(outcome)
}

pub fn kernel_payloads(state: State<'_, AppState>) -> AppResult<Vec<RuleSetKernelPayload>> {
    let items = lock(state.rule_sets(), "rule_set")?;
    items
        .iter()
        .filter(|item| item.enabled)
        .map(|item| {
            let artifact = item.artifact.as_ref().ok_or_else(|| {
                AppError::invalid_argument(format!(
                    "rule asset '{}' has no verified ZRS artifact",
                    item.name
                ))
            })?;
            Ok(RuleSetKernelPayload {
                id: item.id.clone(),
                name: item.name.clone(),
                zrs_path: artifact.path.clone(),
                checksum: artifact.checksum,
            })
        })
        .collect()
}

pub fn remove(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let id = normalize_required(id, "id")?;
    let _in_flight = begin_in_flight(state.rule_set_updates(), "rule_set", &id)?;
    let mut items = lock(state.rule_sets(), "rule_set")?;
    let mut next = items.clone();
    let before = next.len();
    next.retain(|item| item.id != id);
    if next.len() == before {
        return Err(AppError::not_found("rule_set", id));
    }
    // Published generations are intentionally retained: a running kernel may still mmap them.
    domain_store::save_rule_sets(&next)?;
    *items = next;
    Ok(())
}

fn normalize_source(mut source: RuleSetSource) -> AppResult<RuleSetSource> {
    source.url = normalize_required(source.url, "source.url")?;
    if !source.url.starts_with("https://") && !source.url.starts_with("http://") {
        return Err(AppError::invalid_argument(
            "source.url must use http:// or https://",
        ));
    }
    source.format = match source.format.trim().to_ascii_lowercase().as_str() {
        "" | "auto" => "auto".into(),
        "zero" | "zero-rule" | "zero-rule-ir-v1" => "zero-rule-ir-v1".into(),
        "clash" | "clash-yaml" | "clash-classical-yaml" => "clash-classical-yaml".into(),
        other => {
            return Err(AppError::invalid_argument(format!(
                "unsupported subscription adapter: {other}"
            )))
        }
    };
    source.update_interval_secs = match source.update_interval_secs {
        None | Some(0) => None,
        Some(seconds) if seconds < MIN_UPDATE_INTERVAL_SECS => {
            return Err(AppError::invalid_argument(format!(
                "rule update interval must be at least {MIN_UPDATE_INTERVAL_SECS} seconds"
            )))
        }
        value => value,
    };
    source.user_agent = normalize_optional(source.user_agent);
    Ok(source)
}

struct FetchedResource {
    content: String,
    state: RuleSetSourceState,
}

enum FetchOutcome {
    Modified(FetchedResource),
    NotModified(RuleSetSourceState),
}

async fn fetch_source(
    source: &RuleSetSource,
    previous: Option<&RuleSetSourceState>,
) -> AppResult<FetchOutcome> {
    let url = source.url.clone();
    let user_agent = source
        .user_agent
        .clone()
        .unwrap_or_else(|| DEFAULT_USER_AGENT.to_string());
    let previous = previous.cloned().unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .no_proxy()
            .user_agent(user_agent)
            .build()
            .map_err(|error| AppError::internal(format!("failed to build rule client: {error}")))?;
        let mut request = client.get(&url);
        if let Some(etag) = &previous.etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = &previous.last_modified {
            request = request.header(reqwest::header::IF_MODIFIED_SINCE, last_modified);
        }
        let mut response = request.send().map_err(|error| {
            AppError::internal(format!("failed to download rule source: {error}"))
        })?;
        let checked_at = now_unix_ms();
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            let mut state = previous;
            state.last_checked_at_unix_ms = Some(checked_at);
            return Ok(FetchOutcome::NotModified(state));
        }
        response = response.error_for_status().map_err(|error| {
            AppError::invalid_argument(format!("rule source rejected update: {error}"))
        })?;
        if response
            .content_length()
            .is_some_and(|size| size > MAX_INPUT_BYTES as u64)
        {
            return Err(AppError::invalid_argument("rule source exceeds 64 MiB"));
        }
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let last_modified = response
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let mut bytes = Vec::new();
        response
            .take(MAX_INPUT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                AppError::invalid_argument(format!("failed to read rule source: {error}"))
            })?;
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(AppError::invalid_argument("rule source exceeds 64 MiB"));
        }
        let content_bytes = bytes.len() as u64;
        let content_sha256 = format!("{:x}", Sha256::digest(&bytes));
        let content = String::from_utf8(bytes).map_err(|error| {
            AppError::invalid_argument(format!("rule source is not valid UTF-8: {error}"))
        })?;
        let state = RuleSetSourceState {
            etag,
            last_modified,
            content_sha256: Some(content_sha256.clone()),
            content_bytes: Some(content_bytes),
            last_checked_at_unix_ms: Some(checked_at),
        };
        if previous.content_sha256.as_deref() == Some(content_sha256.as_str()) {
            return Ok(FetchOutcome::NotModified(state));
        }
        Ok(FetchOutcome::Modified(FetchedResource { content, state }))
    })
    .await
    .map_err(|error| AppError::internal(format!("rule download worker failed: {error}")))?
}

pub fn spawn_auto_update_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(15)).await;
        let mut retry_states = HashMap::new();
        loop {
            run_auto_update_pass(&app, &mut retry_states).await;
            tokio::time::sleep(Duration::from_secs(AUTO_UPDATE_TICK_SECS)).await;
        }
    });
}

async fn run_auto_update_pass(
    app: &AppHandle,
    retry_states: &mut HashMap<String, AutoUpdateRetryState>,
) {
    let state = app.state::<AppState>();
    let now = now_unix_ms();
    let due = match collect_due_rule_set_attempts(state.inner(), retry_states, now) {
        Ok(attempts) => attempts,
        Err(error) => {
            logs::znet_log_fields(
                Some(state.inner()),
                LogLevel::Warn,
                format!(
                    "rule-set auto-update: failed to collect due assets: {}",
                    error.message
                ),
                json!({
                    "schema": "znet.rule-set-update.v1",
                    "operation": "collect_due",
                    "errorCode": error.code,
                    "errorMessage": error.message,
                }),
            );
            return;
        }
    };
    for attempt in due {
        let attempt_label = match &attempt.kind {
            AutoUpdateAttemptKind::Initial => "scheduled attempt".to_string(),
            AutoUpdateAttemptKind::Retry { number } => {
                format!("retry {number}/{AUTO_UPDATE_MAX_RETRIES}")
            }
        };
        match update_by_id(state.inner(), attempt.id.clone()).await {
            Ok((profile, changed)) => {
                retry_states.remove(&attempt.id);
                logs::znet_log_fields(
                    Some(state.inner()),
                    LogLevel::Info,
                    format!(
                        "rule-set auto-update: '{}' {}",
                        profile.name,
                        if changed {
                            "published a new ZRS generation"
                        } else {
                            "is unchanged"
                        }
                    ),
                    json!({
                        "schema": "znet.rule-set-update.v1",
                        "operation": "auto_update",
                        "ruleSetId": attempt.id,
                        "ruleSetName": profile.name,
                        "attempt": attempt_label,
                        "outcome": if changed { "updated" } else { "unchanged" },
                    }),
                );
            }
            Err(error) if error.code == "conflict" => {
                logs::znet_log_fields(
                    Some(state.inner()),
                    LogLevel::Debug,
                    format!(
                        "rule-set auto-update: skipped '{}' because an update is already running",
                        attempt.name
                    ),
                    json!({
                        "schema": "znet.rule-set-update.v1",
                        "operation": "auto_update",
                        "ruleSetId": attempt.id,
                        "ruleSetName": attempt.name,
                        "attempt": attempt_label,
                        "outcome": "skipped_conflict",
                    }),
                );
            }
            Err(error) => {
                let disposition = record_auto_update_failure(retry_states, &attempt, now_unix_ms());
                let (message, retry_number, delay_secs, retry_exhausted) = match disposition {
                    AutoUpdateFailureDisposition::RetryScheduled {
                        retry_number,
                        delay_secs,
                        ..
                    } => (
                        format!(
                            "rule-set auto-update: '{}' {attempt_label} failed: {}; retry {retry_number}/{AUTO_UPDATE_MAX_RETRIES} in {delay_secs}s",
                            attempt.name, error.message,
                        ),
                        retry_number,
                        delay_secs,
                        false,
                    ),
                    AutoUpdateFailureDisposition::RetryLimitReached { cooldown_secs, .. } => (
                        format!(
                            "rule-set auto-update: '{}' {attempt_label} failed after {AUTO_UPDATE_MAX_RETRIES} retries: {}; automatic retries paused for {cooldown_secs}s",
                            attempt.name, error.message,
                        ),
                        AUTO_UPDATE_MAX_RETRIES,
                        cooldown_secs,
                        true,
                    ),
                };
                logs::znet_log_fields(
                    Some(state.inner()),
                    LogLevel::Warn,
                    message,
                    json!({
                        "schema": "znet.rule-set-update.v1",
                        "operation": "auto_update",
                        "ruleSetId": attempt.id,
                        "ruleSetName": attempt.name,
                        "attempt": attempt_label,
                        "outcome": "failure",
                        "errorCode": error.code,
                        "errorMessage": error.message,
                        "retryNumber": retry_number,
                        "retryLimit": AUTO_UPDATE_MAX_RETRIES,
                        "nextDelaySecs": delay_secs,
                        "retryExhausted": retry_exhausted,
                    }),
                );
            }
        }
    }
}

fn collect_due_rule_set_attempts(
    state: &AppState,
    retry_states: &mut HashMap<String, AutoUpdateRetryState>,
    now_unix_ms: u64,
) -> AppResult<Vec<DueRuleSet>> {
    let items = lock(state.rule_sets(), "rule_set")?;
    retry_states.retain(|id, retry| {
        items.iter().any(|item| {
            item.id == *id
                && item.enabled
                && item
                    .source
                    .as_ref()
                    .and_then(|source| source.update_interval_secs)
                    .is_some()
                && item.updated_at_unix_ms == retry.cycle_updated_at_unix_ms
                && item.last_error.is_some()
        })
    });

    let mut reset_cycles = Vec::new();
    let mut due = Vec::new();
    for item in items.iter() {
        let Some(source) = item.source.as_ref() else {
            continue;
        };
        let Some(interval_secs) = source.update_interval_secs else {
            continue;
        };
        if !item.enabled || is_in_flight(state.rule_set_updates(), &item.id)? {
            continue;
        }

        let kind = if let Some(retry) = retry_states.get(&item.id) {
            if now_unix_ms < retry.next_attempt_at_unix_ms {
                continue;
            }
            if retry.failed_attempts > AUTO_UPDATE_MAX_RETRIES {
                reset_cycles.push(item.id.clone());
                AutoUpdateAttemptKind::Initial
            } else {
                AutoUpdateAttemptKind::Retry {
                    number: retry.failed_attempts,
                }
            }
        } else {
            if !is_due(item, now_unix_ms) {
                continue;
            }
            AutoUpdateAttemptKind::Initial
        };

        due.push(DueRuleSet {
            id: item.id.clone(),
            name: item.name.clone(),
            interval_secs,
            updated_at_unix_ms: item.updated_at_unix_ms,
            kind,
        });
    }
    drop(items);
    for id in reset_cycles {
        retry_states.remove(&id);
    }
    Ok(due)
}

fn auto_update_retry_delay_secs(retry_number: u32) -> u64 {
    let exponent = retry_number.saturating_sub(1).min(31);
    AUTO_UPDATE_RETRY_BASE_SECS
        .saturating_mul(1_u64 << exponent)
        .min(AUTO_UPDATE_RETRY_MAX_SECS)
}

fn record_auto_update_failure(
    retry_states: &mut HashMap<String, AutoUpdateRetryState>,
    attempt: &DueRuleSet,
    failed_at_unix_ms: u64,
) -> AutoUpdateFailureDisposition {
    let failed_attempts = retry_states
        .get(&attempt.id)
        .map(|retry| retry.failed_attempts)
        .unwrap_or(0)
        .saturating_add(1);

    if failed_attempts <= AUTO_UPDATE_MAX_RETRIES {
        let delay_secs = auto_update_retry_delay_secs(failed_attempts);
        let next_attempt_at_unix_ms =
            failed_at_unix_ms.saturating_add(delay_secs.saturating_mul(1_000));
        retry_states.insert(
            attempt.id.clone(),
            AutoUpdateRetryState {
                failed_attempts,
                next_attempt_at_unix_ms,
                cycle_updated_at_unix_ms: attempt.updated_at_unix_ms,
            },
        );
        AutoUpdateFailureDisposition::RetryScheduled {
            retry_number: failed_attempts,
            delay_secs,
            next_attempt_at_unix_ms,
        }
    } else {
        let cooldown_secs = attempt.interval_secs.max(AUTO_UPDATE_TICK_SECS);
        let next_cycle_at_unix_ms =
            failed_at_unix_ms.saturating_add(cooldown_secs.saturating_mul(1_000));
        retry_states.insert(
            attempt.id.clone(),
            AutoUpdateRetryState {
                failed_attempts,
                next_attempt_at_unix_ms: next_cycle_at_unix_ms,
                cycle_updated_at_unix_ms: attempt.updated_at_unix_ms,
            },
        );
        AutoUpdateFailureDisposition::RetryLimitReached {
            cooldown_secs,
            next_cycle_at_unix_ms,
        }
    }
}

fn is_due(item: &RuleSetProfile, now_ms: u64) -> bool {
    let Some(source) = &item.source else {
        return false;
    };
    let Some(interval) = source.update_interval_secs else {
        return false;
    };
    if !item.enabled {
        return false;
    }
    let last = item
        .source_state
        .last_checked_at_unix_ms
        .or(item.last_sync_at_unix_ms)
        .unwrap_or(0);
    now_ms.saturating_sub(last) >= interval.saturating_mul(1000)
}

fn canonical_ir(value: Value, display_name: &str) -> AppResult<Value> {
    let mut value = value;
    value["name"] = json!(display_name);
    let bytes = serde_json::to_vec(&value)
        .map_err(|error| AppError::invalid_argument(format!("invalid semantic rules: {error}")))?;
    let rule_set = decode_json(&bytes)
        .map_err(|error| AppError::invalid_argument(format!("invalid Zero Rule IR: {error}")))?;
    let canonical = encode_json(&rule_set)
        .map_err(|error| AppError::internal(format!("failed to encode Zero Rule IR: {error}")))?;
    serde_json::from_slice(&canonical)
        .map_err(|error| AppError::internal(format!("failed to materialize Zero Rule IR: {error}")))
}

pub fn convert_source(content: &str, format: &str, display_name: &str) -> AppResult<Value> {
    if content.len() > MAX_INPUT_BYTES {
        return Err(AppError::invalid_argument("rule source exceeds 64 MiB"));
    }
    match format {
        "zero-rule-ir-v1" => {
            let value = serde_json::from_str(content).map_err(|error| {
                AppError::invalid_argument(format!("Zero Rule IR JSON is invalid: {error}"))
            })?;
            canonical_ir(value, display_name)
        }
        "clash-classical-yaml" => convert_clash(content, display_name),
        "auto" => serde_json::from_str(content)
            .ok()
            .and_then(|value| canonical_ir(value, display_name).ok())
            .map(Ok)
            .unwrap_or_else(|| convert_clash(content, display_name)),
        other => Err(AppError::invalid_argument(format!(
            "unsupported subscription adapter: {other}"
        ))),
    }
}

fn convert_clash(content: &str, display_name: &str) -> AppResult<Value> {
    let yaml: Value = serde_yaml::from_str(content).map_err(|error| {
        AppError::invalid_argument(format!("Clash rule YAML is invalid: {error}"))
    })?;
    let payload = yaml
        .get("payload")
        .and_then(Value::as_array)
        .or_else(|| yaml.as_array())
        .ok_or_else(|| AppError::invalid_argument("Clash rules must contain a payload array"))?;
    let mut rules = Vec::with_capacity(payload.len());
    for entry in payload {
        let raw = entry
            .as_str()
            .ok_or_else(|| AppError::invalid_argument("Clash payload entries must be strings"))?;
        let mut parts = raw.split(',').map(str::trim);
        let source_type = parts.next().unwrap_or("").to_ascii_uppercase();
        let value = parts
            .next()
            .ok_or_else(|| AppError::invalid_argument(format!("Clash rule has no value: {raw}")))?;
        let rule_type = match source_type.as_str() {
            "DOMAIN" => "domain_exact",
            "DOMAIN-SUFFIX" => "domain_suffix",
            "DOMAIN-KEYWORD" => "domain_keyword",
            "IP-CIDR" => "ipv4_cidr",
            "IP-CIDR6" => "ipv6_cidr",
            _ => {
                return Err(AppError::invalid_argument(format!(
                    "unsupported Clash rule type '{source_type}'; conversion is strict"
                )))
            }
        };
        rules.push(json!({ "type": rule_type, "value": value }));
    }
    canonical_ir(
        json!({ "version": 1, "name": display_name, "rules": rules }),
        display_name,
    )
}

fn build_zrs_artifact(id: &str, semantic_ir: &Value) -> AppResult<ZrsArtifact> {
    let ir = serde_json::to_vec(semantic_ir).map_err(|error| {
        AppError::internal(format!("failed to serialize semantic rules: {error}"))
    })?;
    let source = decode_json(&ir)
        .map_err(|error| AppError::invalid_argument(format!("invalid Zero Rule IR: {error}")))?;
    let (compiled, _) = RuleSetCompiler
        .compile(source)
        .map_err(|error| AppError::invalid_argument(format!("rule compilation failed: {error}")))?;
    let bytes = encode(&compiled)
        .map_err(|error| AppError::internal(format!("ZRS encoding failed: {error}")))?;
    let metadata = verify(&bytes, VerifyMode::FullChecksum)
        .map_err(|error| AppError::internal(format!("ZRS verification failed: {error}")))?;
    publish_zrs_in(&data_dir()?, id, &bytes, metadata)
}

fn publish_zrs_in(
    base_dir: &Path,
    id: &str,
    bytes: &[u8],
    metadata: RuleSetMetadata,
) -> AppResult<ZrsArtifact> {
    let directory = base_dir.join("rule-artifacts").join(id);
    fs::create_dir_all(&directory)
        .map_err(|error| io_error("create ZRS directory", &directory, error))?;
    let generation = now_unix_ms();
    let file_name = format!("{generation}-{:08x}.zrs", metadata.body_checksum);
    let final_path = directory.join(file_name);
    let temporary_path = directory.join(format!(".{generation}-{}.tmp", std::process::id()));
    let publish = || -> AppResult<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
            .map_err(|error| io_error("create temporary ZRS", &temporary_path, error))?;
        file.write_all(bytes)
            .map_err(|error| io_error("write temporary ZRS", &temporary_path, error))?;
        file.sync_all()
            .map_err(|error| io_error("flush temporary ZRS", &temporary_path, error))?;
        let written = fs::read(&temporary_path)
            .map_err(|error| io_error("read temporary ZRS", &temporary_path, error))?;
        verify(&written, VerifyMode::FullChecksum).map_err(|error| {
            AppError::internal(format!("published ZRS verification failed: {error}"))
        })?;
        fs::rename(&temporary_path, &final_path)
            .map_err(|error| io_error("publish immutable ZRS", &final_path, error))?;
        Ok(())
    };
    if let Err(error) = publish() {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    Ok(ZrsArtifact {
        path: final_path.to_string_lossy().into_owned(),
        major_version: metadata.major_version,
        minor_version: metadata.minor_version,
        checksum: metadata.body_checksum,
        file_size: metadata.file_size,
        entry_count: metadata.entry_count(),
        built_at_unix_ms: generation,
    })
}

fn io_error(action: &str, path: &Path, error: std::io::Error) -> AppError {
    AppError {
        code: "io_error",
        message: format!("failed to {action}: {error}"),
        details: Some(json!({ "path": path.display().to_string() })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::app_config::AppConfig;

    fn scheduled_profile() -> RuleSetProfile {
        RuleSetProfile {
            id: "asset".into(),
            name: "Asset".into(),
            enabled: true,
            semantic_ir: json!({"version":1,"name":"Asset","rules":[]}),
            source: Some(RuleSetSource {
                url: "https://example.com/rules".into(),
                format: "auto".into(),
                update_interval_secs: Some(3600),
                user_agent: None,
            }),
            source_state: RuleSetSourceState {
                last_checked_at_unix_ms: Some(1_000),
                ..Default::default()
            },
            artifact: None,
            updated_at_unix_ms: 1_000,
            last_sync_at_unix_ms: Some(1_000),
            last_error: None,
        }
    }

    #[test]
    fn clash_subscription_becomes_canonical_kernel_semantics() {
        let ir = convert_source(
            "payload:\n  - DOMAIN,Api.Example.COM.\n  - IP-CIDR,10.0.0.0/8,no-resolve\n",
            "clash-classical-yaml",
            "Imported",
        )
        .unwrap();
        assert_eq!(ir["version"], 1);
        assert_eq!(ir["rules"].as_array().unwrap().len(), 2);
        assert!(!ir.to_string().contains("Clash"));
        assert!(!ir.to_string().contains("no-resolve"));
    }

    #[test]
    fn unsupported_subscription_semantics_are_rejected() {
        let error =
            convert_source("payload:\n  - GEOIP,CN\n", "clash-classical-yaml", "Bad").unwrap_err();
        assert!(error.message.contains("unsupported Clash rule type"));
    }

    #[test]
    fn semantic_rules_compile_to_verified_zrs() {
        let ir = canonical_ir(
            json!({
                "version": 1,
                "rules": [
                    {"type":"domain_suffix","value":"example.com"},
                    {"type":"ipv4_cidr","value":"10.0.0.0/8"}
                ]
            }),
            "Visual asset",
        )
        .unwrap();
        let source = decode_json(&serde_json::to_vec(&ir).unwrap()).unwrap();
        let (compiled, _) = RuleSetCompiler.compile(source).unwrap();
        let bytes = encode(&compiled).unwrap();
        let metadata = verify(&bytes, VerifyMode::FullChecksum).unwrap();
        assert_eq!(&bytes[..4], b"ZRS!");
        assert_eq!(metadata.entry_count(), 2);
    }

    #[test]
    fn zrs_generation_is_published_as_a_verified_immutable_file() {
        let root = std::env::temp_dir().join(format!(
            "znet-rule-test-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        let ir = canonical_ir(
            json!({"version":1,"rules":[{"type":"domain_exact","value":"example.com"}]}),
            "Published",
        )
        .unwrap();
        let source = decode_json(&serde_json::to_vec(&ir).unwrap()).unwrap();
        let (compiled, _) = RuleSetCompiler.compile(source).unwrap();
        let bytes = encode(&compiled).unwrap();
        let metadata = verify(&bytes, VerifyMode::FullChecksum).unwrap();
        let artifact = publish_zrs_in(&root, "asset", &bytes, metadata).unwrap();
        let published = fs::read(&artifact.path).unwrap();
        verify(&published, VerifyMode::FullChecksum).unwrap();
        assert_eq!(&published[..4], b"ZRS!");
        assert!(!Path::new(&artifact.path).with_extension("tmp").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn auto_update_only_selects_due_enabled_external_assets() {
        let mut profile = scheduled_profile();
        assert!(!is_due(&profile, 3_600_999));
        assert!(is_due(&profile, 3_601_000));
        profile.enabled = false;
        assert!(!is_due(&profile, u64::MAX));
    }

    #[test]
    fn auto_update_retry_delay_is_exponential_and_capped() {
        assert_eq!(auto_update_retry_delay_secs(1), 60);
        assert_eq!(auto_update_retry_delay_secs(2), 120);
        assert_eq!(auto_update_retry_delay_secs(3), 240);
        assert_eq!(auto_update_retry_delay_secs(8), AUTO_UPDATE_RETRY_MAX_SECS);
    }

    #[test]
    fn auto_update_retry_cycle_is_bounded_and_cools_down() {
        let mut profile = scheduled_profile();
        profile.last_error = Some("offline".into());
        let state = AppState::with_domain_data(
            AppConfig::default(),
            Vec::new(),
            Vec::new(),
            vec![profile],
            Vec::new(),
        );
        let mut retries = HashMap::new();
        let mut now = 3_601_000;
        let initial = collect_due_rule_set_attempts(&state, &mut retries, now)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(initial.kind, AutoUpdateAttemptKind::Initial);

        for retry_number in 1..=AUTO_UPDATE_MAX_RETRIES {
            let outcome = record_auto_update_failure(&mut retries, &initial, now);
            let AutoUpdateFailureDisposition::RetryScheduled {
                retry_number: scheduled_number,
                next_attempt_at_unix_ms,
                ..
            } = outcome
            else {
                panic!("retry {retry_number} should be scheduled");
            };
            assert_eq!(scheduled_number, retry_number);
            assert!(collect_due_rule_set_attempts(
                &state,
                &mut retries,
                next_attempt_at_unix_ms - 1,
            )
            .unwrap()
            .is_empty());
            now = next_attempt_at_unix_ms;
            let retry = collect_due_rule_set_attempts(&state, &mut retries, now)
                .unwrap()
                .pop()
                .unwrap();
            assert_eq!(
                retry.kind,
                AutoUpdateAttemptKind::Retry {
                    number: retry_number,
                }
            );
        }

        let exhausted = record_auto_update_failure(&mut retries, &initial, now);
        let AutoUpdateFailureDisposition::RetryLimitReached {
            next_cycle_at_unix_ms,
            ..
        } = exhausted
        else {
            panic!("retry budget should be exhausted");
        };
        assert!(
            collect_due_rule_set_attempts(&state, &mut retries, next_cycle_at_unix_ms - 1,)
                .unwrap()
                .is_empty()
        );
        let next_cycle = collect_due_rule_set_attempts(&state, &mut retries, next_cycle_at_unix_ms)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(next_cycle.kind, AutoUpdateAttemptKind::Initial);
    }
}
