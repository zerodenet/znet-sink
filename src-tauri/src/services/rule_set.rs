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
    RuleSetKernelPayload, RuleSetProfile, RuleSetSource, RuleSetSourceState, RuleSetSummary,
    RuleSetSyncAllOutcome, RuleSetUpsert, ZrsArtifact,
};
use crate::services::common::{
    begin_in_flight, generated_store_id, is_in_flight, lock, normalize_optional,
    normalize_required, now_unix_ms,
};
use crate::services::{data_dir, domain_store, logs, rule_overlay};
use crate::state::app_state::AppState;

const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_LOCAL_VISUAL_RULES: usize = 1_000;
const MIN_UPDATE_INTERVAL_SECS: u64 = 60;
const AUTO_UPDATE_TICK_SECS: u64 = 60;
const AUTO_UPDATE_MAX_RETRIES: u32 = 3;
const AUTO_UPDATE_RETRY_BASE_SECS: u64 = 60;
const AUTO_UPDATE_RETRY_MAX_SECS: u64 = 15 * 60;
const DEFAULT_USER_AGENT: &str = concat!("ZNet-Sink/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug)]
pub(crate) struct ManagedRuleSetSource {
    pub tag: String,
    pub url: String,
    pub format: String,
    pub update_interval_secs: Option<u64>,
    pub user_agent: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ManagedRuleSetArtifact {
    pub tag: String,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ManagedRuleSetFailure {
    pub tag: String,
    pub message: String,
    pub used_previous_artifact: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ManagedRuleSetSyncOutcome {
    pub artifacts: Vec<ManagedRuleSetArtifact>,
    pub failures: Vec<ManagedRuleSetFailure>,
}

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

pub fn list(state: State<'_, AppState>) -> AppResult<Vec<RuleSetSummary>> {
    // Subscription-owned providers are implementation details of subscription
    // conversion. They share persistence with rule assets, but are not GUI
    // resources and must not cross the rule-set command boundary.
    Ok(gui_owned_rule_sets(&lock(state.rule_sets(), "rule_set")?))
}

pub fn get(state: State<'_, AppState>, id: String) -> AppResult<RuleSetProfile> {
    let id = normalize_required(id, "id")?;
    lock(state.rule_sets(), "rule_set")?
        .iter()
        .find(|item| item.id == id && !is_subscription_managed_rule_set(item))
        .cloned()
        .ok_or_else(|| AppError::not_found("rule_set", id))
}

pub async fn upsert(state: State<'_, AppState>, input: RuleSetUpsert) -> AppResult<RuleSetProfile> {
    let name = normalize_required(input.name, "name")?;
    let id = normalize_optional(input.id).unwrap_or_else(|| generated_store_id("rule-set"));
    let _in_flight = begin_in_flight(state.rule_set_updates(), "rule_set", &id)?;
    let previous_profile = lock(state.rule_sets(), "rule_set")?
        .iter()
        .find(|item| item.id == id)
        .cloned();
    if previous_profile.as_ref().is_some_and(|item| item.built_in) {
        return Err(AppError::invalid_argument(
            "built-in rules can be rebound or disabled, but not edited",
        ));
    }
    if previous_profile
        .as_ref()
        .is_some_and(is_subscription_managed_rule_set)
    {
        return Err(AppError::invalid_argument(
            "subscription-managed rules must be changed through their subscription",
        ));
    }
    let previous = previous_profile.as_ref().map(|item| {
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
                convert_source(resource_text(&resource)?, &source.format, &name)?,
                resource.state,
                true,
            )
        }
    };
    if source.is_none()
        && semantic_ir
            .get("rules")
            .and_then(Value::as_array)
            .is_some_and(|rules| rules.len() > MAX_LOCAL_VISUAL_RULES)
    {
        return Err(AppError::invalid_argument(format!(
            "local visual rule sets are limited to {MAX_LOCAL_VISUAL_RULES} entries; use an external source for larger assets"
        )));
    }
    let artifact = build_zrs_artifact(&id, &semantic_ir)?;
    let stored_semantic_ir = if source.is_some() {
        empty_semantic_ir(&name)
    } else {
        semantic_ir
    };
    let now = now_unix_ms();
    let profile = RuleSetProfile {
        id: id.clone(),
        name,
        enabled: input.enabled.unwrap_or(true),
        built_in: false,
        provenance: None,
        managed_by_subscription_id: previous_profile
            .as_ref()
            .and_then(|item| item.managed_by_subscription_id.clone()),
        common_binding: previous_profile
            .as_ref()
            .and_then(|item| item.common_binding.clone()),
        semantic_ir: stored_semantic_ir,
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
    if is_subscription_managed_rule_set(&profile) {
        return Err(AppError::invalid_argument(
            "subscription-managed rules must be updated through their subscription",
        ));
    }
    let source = profile
        .source
        .clone()
        .ok_or_else(|| AppError::invalid_argument("this rule asset has no subscription source"))?;
    let result = async {
        match fetch_source(&source, Some(&profile.source_state)).await? {
            FetchOutcome::NotModified(source_state) => Ok::<_, AppError>((None, source_state)),
            FetchOutcome::Modified(resource) => {
                let semantic_ir =
                    convert_source(resource_text(&resource)?, &source.format, &profile.name)?;
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
                item.semantic_ir = if item.source.is_some() {
                    empty_semantic_ir(&item.name)
                } else {
                    semantic_ir
                };
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
        .filter(|item| {
            item.enabled && item.source.is_some() && !is_subscription_managed_rule_set(item)
        })
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

/// Synchronize the rule providers owned by one proxy subscription and return
/// the verified ZRS files that must be wired into `route.rule_sets`.
///
/// The provider files are prepared first and the domain store is replaced only
/// after every provider succeeds. Published ZRS generations are immutable, so
/// an interrupted batch can leave an unreferenced generation on disk without
/// invalidating the last known-good configuration.
pub(crate) async fn sync_managed_subscription_sources(
    state: &AppState,
    subscription_id: &str,
    subscription_name: &str,
    sources: Vec<ManagedRuleSetSource>,
) -> AppResult<ManagedRuleSetSyncOutcome> {
    let id_prefix = managed_rule_set_id_prefix(subscription_id);
    let current = lock(state.rule_sets(), "rule_set")?.clone();
    let now = now_unix_ms();
    let mut prepared = Vec::with_capacity(sources.len());
    let mut artifacts = Vec::with_capacity(sources.len());
    let mut failures = Vec::new();

    for managed in sources {
        let id = managed_rule_set_id(&id_prefix, &managed.tag);
        let raw_source = RuleSetSource {
            url: managed.url,
            format: managed.format,
            update_interval_secs: managed.update_interval_secs,
            user_agent: managed.user_agent,
        };
        let previous = current.iter().find(|item| item.id == id);
        let source = match normalize_managed_source(raw_source.clone()) {
            Ok(source) => source,
            Err(error) => {
                push_managed_source_failure(
                    &mut prepared,
                    &mut artifacts,
                    &mut failures,
                    previous,
                    id,
                    subscription_id,
                    subscription_name,
                    &managed.tag,
                    raw_source,
                    error,
                    now,
                );
                continue;
            }
        };
        let same_source = previous
            .and_then(|item| item.source.as_ref())
            .is_some_and(|old| old.url == source.url && old.format == source.format);
        let previous_state = same_source
            .then(|| previous.map(|item| &item.source_state))
            .flatten();

        let result = async {
            let values = match fetch_source(&source, previous_state).await? {
                FetchOutcome::Modified(resource) => {
                    let display_name = format!("{subscription_name} / {}", managed.tag);
                    let artifact = if source.format == "zrs" {
                        let metadata =
                            verify(&resource.bytes, VerifyMode::FullChecksum).map_err(|error| {
                                AppError::invalid_argument(format!(
                                    "downloaded ZRS for '{}' is invalid: {error}",
                                    managed.tag
                                ))
                            })?;
                        publish_zrs_in(&data_dir()?, &id, &resource.bytes, metadata)?
                    } else {
                        let semantic_ir =
                            convert_managed_clash_source(resource_text(&resource)?, &display_name)?;
                        build_zrs_artifact(&id, &semantic_ir)?
                    };
                    (artifact, resource.state, Some(now), now)
                }
                FetchOutcome::NotModified(source_state) => {
                    let previous = previous.ok_or_else(|| {
                        AppError::internal(
                            "managed rule source returned not-modified without a stored profile",
                        )
                    })?;
                    let artifact = previous.artifact.clone().ok_or_else(|| {
                        AppError::invalid_argument(format!(
                            "managed rule source '{}' has no verified ZRS artifact",
                            managed.tag
                        ))
                    })?;
                    (
                        artifact,
                        source_state,
                        previous.last_sync_at_unix_ms,
                        previous.updated_at_unix_ms,
                    )
                }
            };
            Ok::<_, AppError>(values)
        }
        .await;

        let (artifact, source_state, last_sync_at_unix_ms, updated_at_unix_ms) = match result {
            Ok(values) => values,
            Err(error) => {
                push_managed_source_failure(
                    &mut prepared,
                    &mut artifacts,
                    &mut failures,
                    previous,
                    id,
                    subscription_id,
                    subscription_name,
                    &managed.tag,
                    source,
                    error,
                    now,
                );
                continue;
            }
        };

        artifacts.push(ManagedRuleSetArtifact {
            tag: managed.tag.clone(),
            path: artifact.path.clone(),
        });
        prepared.push(RuleSetProfile {
            id,
            name: format!("{subscription_name} / {}", managed.tag),
            enabled: true,
            built_in: false,
            provenance: None,
            managed_by_subscription_id: Some(subscription_id.to_string()),
            common_binding: None,
            semantic_ir: empty_managed_semantic_ir(subscription_name, &managed.tag),
            source: Some(source),
            source_state,
            artifact: Some(artifact),
            updated_at_unix_ms,
            last_sync_at_unix_ms,
            last_error: None,
        });
    }

    let mut next = current;
    next.retain(|item| !item.id.starts_with(&id_prefix));
    next.extend(prepared);
    domain_store::save_rule_sets(&next)?;
    *lock(state.rule_sets(), "rule_set")? = next;
    Ok(ManagedRuleSetSyncOutcome {
        artifacts,
        failures,
    })
}

#[allow(clippy::too_many_arguments)]
fn push_managed_source_failure(
    prepared: &mut Vec<RuleSetProfile>,
    artifacts: &mut Vec<ManagedRuleSetArtifact>,
    failures: &mut Vec<ManagedRuleSetFailure>,
    previous: Option<&RuleSetProfile>,
    id: String,
    subscription_id: &str,
    subscription_name: &str,
    tag: &str,
    source: RuleSetSource,
    error: AppError,
    now: u64,
) {
    let mut profile = previous.cloned().unwrap_or_else(|| RuleSetProfile {
        id,
        name: format!("{subscription_name} / {tag}"),
        enabled: true,
        built_in: false,
        provenance: None,
        managed_by_subscription_id: Some(subscription_id.to_string()),
        common_binding: None,
        semantic_ir: json!({
            "version": 1,
            "name": format!("{subscription_name} / {tag}"),
            "rules": []
        }),
        source: Some(source.clone()),
        source_state: RuleSetSourceState::default(),
        artifact: None,
        updated_at_unix_ms: now,
        last_sync_at_unix_ms: None,
        last_error: None,
    });
    profile.name = format!("{subscription_name} / {tag}");
    profile.enabled = true;
    profile.managed_by_subscription_id = Some(subscription_id.to_string());
    profile.common_binding = None;
    profile.semantic_ir = empty_managed_semantic_ir(subscription_name, tag);
    if profile.source.as_ref().is_none_or(|previous_source| {
        previous_source.url != source.url || previous_source.format != source.format
    }) {
        profile.source_state = RuleSetSourceState::default();
    }
    profile.source = Some(source);
    profile.last_error = Some(error.message.clone());
    profile.source_state.last_checked_at_unix_ms = Some(now);
    let used_previous_artifact = profile.artifact.is_some();
    if let Some(artifact) = profile.artifact.as_ref() {
        artifacts.push(ManagedRuleSetArtifact {
            tag: tag.to_string(),
            path: artifact.path.clone(),
        });
    }
    failures.push(ManagedRuleSetFailure {
        tag: tag.to_string(),
        message: error.message,
        used_previous_artifact,
    });
    prepared.push(profile);
}

fn empty_managed_semantic_ir(subscription_name: &str, tag: &str) -> Value {
    empty_semantic_ir(&format!("{subscription_name} / {tag}"))
}

fn empty_semantic_ir(display_name: &str) -> Value {
    json!({ "version": 1, "name": display_name, "rules": [] })
}

fn managed_rule_set_id_prefix(subscription_id: &str) -> String {
    format!("subscription-rule-{subscription_id}-")
}

pub(crate) fn is_managed_subscription_rule_set_id(id: &str) -> bool {
    id.starts_with("subscription-rule-")
}

fn is_subscription_managed_rule_set(profile: &RuleSetProfile) -> bool {
    profile.managed_by_subscription_id.is_some() || is_managed_subscription_rule_set_id(&profile.id)
}

fn gui_owned_rule_sets(items: &[RuleSetProfile]) -> Vec<RuleSetSummary> {
    items
        .iter()
        .filter(|item| !is_subscription_managed_rule_set(item))
        .map(RuleSetSummary::from)
        .collect()
}

pub(crate) fn managed_subscription_rule_set_count(
    state: &AppState,
    subscription_id: &str,
) -> AppResult<usize> {
    let id_prefix = managed_rule_set_id_prefix(subscription_id);
    Ok(lock(state.rule_sets(), "rule_set")?
        .iter()
        .filter(|item| {
            item.managed_by_subscription_id.as_deref() == Some(subscription_id)
                || item.id.starts_with(&id_prefix)
        })
        .count())
}

pub(crate) fn remove_managed_subscription_rule_sets(
    state: &AppState,
    subscription_id: &str,
) -> AppResult<usize> {
    let id_prefix = managed_rule_set_id_prefix(subscription_id);
    let mut items = lock(state.rule_sets(), "rule_set")?;
    let mut next = items.clone();
    let before = next.len();
    next.retain(|item| {
        item.managed_by_subscription_id.as_deref() != Some(subscription_id)
            && !item.id.starts_with(&id_prefix)
    });
    let removed = before.saturating_sub(next.len());
    if removed == 0 {
        return Ok(0);
    }
    // Immutable ZRS generations are retained until a later garbage-collection
    // pass because a running kernel may still have one memory-mapped.
    domain_store::save_rule_sets(&next)?;
    *items = next;
    Ok(removed)
}

pub(crate) fn compact_source_managed_semantics(items: &mut [RuleSetProfile]) -> bool {
    let mut changed = false;
    for item in items {
        if item.managed_by_subscription_id.is_none() && item.source.is_none() {
            continue;
        }
        let compact = empty_semantic_ir(&item.name);
        if item.semantic_ir != compact {
            item.semantic_ir = compact;
            changed = true;
        }
    }
    changed
}

fn managed_rule_set_id(prefix: &str, tag: &str) -> String {
    let digest = Sha256::digest(tag.as_bytes());
    let suffix = digest
        .iter()
        .take(12)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{prefix}{suffix}")
}

pub fn kernel_payloads(state: State<'_, AppState>) -> AppResult<Vec<RuleSetKernelPayload>> {
    let items = lock(state.rule_sets(), "rule_set")?;
    items
        .iter()
        .filter(|item| item.enabled && !is_subscription_managed_rule_set(item))
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
    if items
        .iter()
        .any(|item| item.id == id && is_subscription_managed_rule_set(item))
    {
        return Err(AppError::invalid_argument(
            "subscription-managed rules must be removed through their subscription",
        ));
    }
    if items.iter().any(|item| item.id == id && item.built_in) {
        return Err(AppError::invalid_argument(
            "built-in rules can be disabled, but not removed",
        ));
    }
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

fn normalize_managed_source(source: RuleSetSource) -> AppResult<RuleSetSource> {
    if source.format.trim().eq_ignore_ascii_case("zrs") {
        let mut source = source;
        source.url = normalize_required(source.url, "source.url")?;
        if !source.url.starts_with("https://") && !source.url.starts_with("http://") {
            return Err(AppError::invalid_argument(
                "source.url must use http:// or https://",
            ));
        }
        source.format = "zrs".to_string();
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
        return Ok(source);
    }
    normalize_source(source)
}

struct FetchedResource {
    bytes: Vec<u8>,
    state: RuleSetSourceState,
}

fn resource_text(resource: &FetchedResource) -> AppResult<&str> {
    std::str::from_utf8(&resource.bytes).map_err(|error| {
        AppError::invalid_argument(format!("rule source is not valid UTF-8: {error}"))
    })
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
        Ok(FetchOutcome::Modified(FetchedResource { bytes, state }))
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
                if let Err(error) = rule_overlay::reconcile_after_rule_change(app.clone()).await {
                    logs::znet_log_fields(
                        Some(state.inner()),
                        LogLevel::Warn,
                        format!(
                            "rule-set auto-update: '{}' was stored but runtime recomposition failed: {}",
                            profile.name, error.message
                        ),
                        json!({
                            "schema": "znet.rule-set-update.v1",
                            "operation": "recompose_runtime",
                            "ruleSetId": attempt.id,
                            "ruleSetName": profile.name,
                            "errorCode": error.code,
                            "errorMessage": error.message,
                        }),
                    );
                }
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
                && !is_subscription_managed_rule_set(item)
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
        if is_subscription_managed_rule_set(item) {
            continue;
        }
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
    convert_clash_with_policy(content, display_name, false)
}

pub(crate) fn convert_managed_clash_source(content: &str, display_name: &str) -> AppResult<Value> {
    convert_clash_with_policy(content, display_name, true)
}

fn convert_clash_with_policy(
    content: &str,
    display_name: &str,
    skip_process_rules: bool,
) -> AppResult<Value> {
    let yaml: Value = serde_yaml::from_str(content).map_err(|error| {
        AppError::invalid_argument(format!("Clash rule YAML is invalid: {error}"))
    })?;
    let payload = yaml
        .get("payload")
        .and_then(Value::as_array)
        .or_else(|| yaml.as_array())
        .ok_or_else(|| AppError::invalid_argument("Clash rules must contain a payload array"))?;
    let mut rules = Vec::with_capacity(payload.len());
    let mut skipped_process_rules = 0usize;
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
            "PROCESS-NAME" if skip_process_rules => {
                skipped_process_rules += 1;
                continue;
            }
            _ => {
                return Err(AppError::invalid_argument(format!(
                    "unsupported Clash rule type '{source_type}'; conversion is strict"
                )))
            }
        };
        rules.push(json!({ "type": rule_type, "value": value }));
    }
    if skipped_process_rules > 0 {
        crate::services::file_logger::emit(
            "warn",
            "subscription",
            "subscription.rule_provider.unsupported_rules_skipped",
            Some(json!({
                "ruleSet": display_name,
                "ruleType": "PROCESS-NAME",
                "skipped": skipped_process_rules,
                "reason": "Zero Rule IR v1 has no process matcher"
            })),
        );
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
            built_in: false,
            provenance: None,
            managed_by_subscription_id: None,
            common_binding: None,
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
    fn managed_clash_provider_skips_only_process_rules_missing_from_zero_ir_v1() {
        let ir = convert_managed_clash_source(
            "payload:\n  - PROCESS-NAME,Example.exe\n  - DOMAIN-SUFFIX,example.com\n",
            "Managed",
        )
        .unwrap();

        assert_eq!(ir["rules"].as_array().unwrap().len(), 1);
        assert_eq!(ir["rules"][0]["type"], "domain_suffix");
        assert_eq!(ir["rules"][0]["value"], "example.com");
    }

    #[test]
    fn managed_rule_set_ids_are_stable_per_subscription_and_tag() {
        let prefix = managed_rule_set_id_prefix("subscription-1");
        let first = managed_rule_set_id(&prefix, "Ads");

        assert_eq!(first, managed_rule_set_id(&prefix, "Ads"));
        assert_ne!(first, managed_rule_set_id(&prefix, "LAN"));
        assert!(first.starts_with("subscription-rule-subscription-1-"));
    }

    #[test]
    fn gui_rule_list_excludes_subscription_owned_providers() {
        let gui_owned = scheduled_profile();
        let mut explicitly_managed = scheduled_profile();
        explicitly_managed.id = "legacy-provider".into();
        explicitly_managed.managed_by_subscription_id = Some("subscription-1".into());
        let mut prefixed_managed = scheduled_profile();
        prefixed_managed.id = "subscription-rule-subscription-1-abc".into();

        let visible =
            gui_owned_rule_sets(&[explicitly_managed, gui_owned.clone(), prefixed_managed]);

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, gui_owned.id);
    }

    #[test]
    fn rule_list_summary_never_serializes_semantic_rules() {
        let mut profile = scheduled_profile();
        profile.source = None;
        profile.semantic_ir = json!({
            "version": 1,
            "name": "Large local asset",
            "rules": [
                {"type":"domain_exact","value":"one.example"},
                {"type":"domain_exact","value":"two.example"}
            ]
        });

        let summary = RuleSetSummary::from(&profile);
        let value = serde_json::to_value(summary).unwrap();

        assert_eq!(value["editableRuleCount"], 2);
        assert!(value.get("semanticIr").is_none());
    }

    #[test]
    fn startup_compaction_discards_source_managed_semantic_arrays_only() {
        let mut source_managed = scheduled_profile();
        source_managed.semantic_ir = json!({
            "version": 1,
            "name": "External",
            "rules": [{"type":"domain_exact","value":"large.example"}]
        });
        let mut local = scheduled_profile();
        local.id = "local".into();
        local.source = None;
        local.semantic_ir = json!({
            "version": 1,
            "name": "Local",
            "rules": [{"type":"domain_exact","value":"keep.example"}]
        });

        let mut profiles = vec![source_managed, local];
        assert!(compact_source_managed_semantics(&mut profiles));
        assert_eq!(
            profiles[0].semantic_ir["rules"].as_array().unwrap().len(),
            0
        );
        assert_eq!(
            profiles[1].semantic_ir["rules"].as_array().unwrap().len(),
            1
        );
        assert!(!compact_source_managed_semantics(&mut profiles));
    }

    #[test]
    fn managed_provider_failure_reuses_last_verified_artifact() {
        let mut previous = scheduled_profile();
        previous.managed_by_subscription_id = Some("subscription-1".into());
        previous.artifact = Some(ZrsArtifact {
            path: "last-good.zrs".into(),
            major_version: 1,
            minor_version: 0,
            checksum: 7,
            file_size: 10,
            entry_count: 1,
            built_at_unix_ms: 1,
        });
        let source = RuleSetSource {
            url: "https://example.com/new.yaml".into(),
            format: "clash-classical-yaml".into(),
            update_interval_secs: Some(3600),
            user_agent: None,
        };
        let mut prepared = Vec::new();
        let mut artifacts = Vec::new();
        let mut failures = Vec::new();

        push_managed_source_failure(
            &mut prepared,
            &mut artifacts,
            &mut failures,
            Some(&previous),
            previous.id.clone(),
            "subscription-1",
            "Airport",
            "Ads",
            source.clone(),
            AppError::invalid_argument("404 Not Found"),
            2_000,
        );

        assert_eq!(artifacts[0].tag, "Ads");
        assert_eq!(artifacts[0].path, "last-good.zrs");
        assert!(failures[0].used_previous_artifact);
        assert_eq!(prepared[0].source.as_ref().unwrap().url, source.url);
        assert_eq!(prepared[0].last_error.as_deref(), Some("404 Not Found"));
        assert!(prepared[0].semantic_ir["rules"]
            .as_array()
            .unwrap()
            .is_empty());
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
    fn identical_zrs_from_distinct_owners_use_isolated_artifact_paths() {
        let root = std::env::temp_dir().join(format!(
            "znet-rule-isolation-test-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        let ir = canonical_ir(
            json!({"version":1,"rules":[{"type":"domain_exact","value":"example.com"}]}),
            "Shared content",
        )
        .unwrap();
        let source = decode_json(&serde_json::to_vec(&ir).unwrap()).unwrap();
        let (compiled, _) = RuleSetCompiler.compile(source).unwrap();
        let bytes = encode(&compiled).unwrap();

        let manual = publish_zrs_in(
            &root,
            "manual-asset",
            &bytes,
            verify(&bytes, VerifyMode::FullChecksum).unwrap(),
        )
        .unwrap();
        let managed = publish_zrs_in(
            &root,
            "subscription-rule-subscription-1-ai",
            &bytes,
            verify(&bytes, VerifyMode::FullChecksum).unwrap(),
        )
        .unwrap();

        assert_ne!(manual.path, managed.path);
        assert_eq!(
            fs::read(manual.path).unwrap(),
            fs::read(managed.path).unwrap()
        );
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
