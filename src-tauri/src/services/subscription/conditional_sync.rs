//! Private HTTP validators bound to a confirmed ordinary source/material pair.
//! No URI, body, credential or validator is returned to the UI or diagnostics.
use super::*;
use crate::services::app_database::storage::{self, Change};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};
use znet_client_capabilities::network::Validators;
use znet_client_core::capability::{Budget, Lease, Manager, Permission};

const MAX_VALIDATOR_BYTES: usize = 4096;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Cache {
    schema: u32,
    source: String,
    material: String,
    target: String,
    target_revision: u64,
    source_revision: u64,
    last_sync: u64,
    etag: Option<String>,
    last_modified: Option<String>,
}
fn failure() -> AppError {
    AppError {
        code: "subscription_cache_error",
        message: "订阅条件请求记录无法读取、校验或保存；配置材料未因此删除，请检查本地存储后重试"
            .into(),
        details: None,
    }
}
fn digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
fn key(id: &str) -> String {
    digest(id.as_bytes())
}
fn source(subscription: &SubscriptionProfile, user_agent: &str) -> String {
    digest(
        &serde_json::to_vec(&(
            &subscription.id,
            &subscription.url,
            user_agent,
            &subscription.format,
            &subscription.kernel,
        ))
        .expect("string tuple serialization"),
    )
}
fn material(profile: &ProxyConfigProfile) -> AppResult<String> {
    if profile.managed_source.is_some() || profile.kernel != "zero" {
        return Err(failure());
    }
    let content = profile.content.as_ref().ok_or_else(failure)?;
    let bytes = serde_json::to_vec(content).map_err(|_| failure())?;
    // A checksum cannot turn corrupt or non-native material into a valid 304 base.
    zero_config::RuntimeConfig::parse(std::str::from_utf8(&bytes).map_err(|_| failure())?)
        .map_err(|_| failure())?;
    Ok(digest(&bytes))
}
pub(super) fn valid_validator(value: &Option<String>) -> bool {
    value.as_ref().is_none_or(|value| {
        !value.is_empty()
            && value.len() <= MAX_VALIDATOR_BYTES
            && value.bytes().all(|b| b >= 32 && b != 127)
    })
}
impl Cache {
    pub(super) fn confirmed(
        subscription: &SubscriptionProfile,
        user_agent: &str,
        profile: &ProxyConfigProfile,
        validators: &Validators,
    ) -> AppResult<Self> {
        if subscription.managed_source.is_some()
            || subscription.last_error.is_some()
            || subscription.target_proxy_config_id.as_deref() != Some(&profile.id)
            || !valid_validator(&validators.etag)
            || !valid_validator(&validators.last_modified)
        {
            return Err(failure());
        }
        Ok(Self {
            schema: 1,
            source: source(subscription, user_agent),
            material: material(profile)?,
            target: key(&profile.id),
            target_revision: profile.updated_at_unix_ms,
            source_revision: subscription.updated_at_unix_ms,
            last_sync: subscription.last_sync_at_unix_ms.ok_or_else(failure)?,
            etag: validators.etag.clone(),
            last_modified: validators.last_modified.clone(),
        })
    }
    pub(super) fn matches_material(&self, profile: &ProxyConfigProfile) -> bool {
        self.target == key(&profile.id)
            && self.target_revision == profile.updated_at_unix_ms
            && material(profile).is_ok_and(|hash| hash == self.material)
    }
    pub(super) fn matches(
        &self,
        subscription: &SubscriptionProfile,
        user_agent: &str,
        profile: &ProxyConfigProfile,
    ) -> bool {
        self.schema == 1
            && subscription.managed_source.is_none()
            && subscription.last_error.is_none()
            && subscription.target_proxy_config_id.as_deref() == Some(&profile.id)
            && self.source == source(subscription, user_agent)
            && self.target == key(&profile.id)
            && self.source_revision == subscription.updated_at_unix_ms
            && Some(self.last_sync) == subscription.last_sync_at_unix_ms
            && self.matches_material(profile)
            && (self.etag.is_some() || self.last_modified.is_some())
    }
    pub(super) fn validators(&self) -> Validators {
        Validators {
            etag: self.etag.clone(),
            last_modified: self.last_modified.clone(),
        }
    }
}
fn lease(manager: &Manager) -> AppResult<Lease> {
    let grants = BTreeSet::from([
        Permission::new("storage.read", "self"),
        Permission::new("storage.write", "self"),
    ]);
    let policy = manager
        .admit(
            "builtin.subscription.conditionals".into(),
            grants.clone(),
            grants.clone(),
            &grants,
        )
        .map_err(|_| failure())?;
    policy
        .authorize(grants, Duration::from_secs(30))
        .map_err(|_| failure())?;
    policy
        .begin(
            Budget {
                calls: 1,
                resource_bytes: 65544,
                timeout: Duration::from_secs(10),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .map_err(|_| failure())
}
fn decode(bytes: &[u8]) -> AppResult<Cache> {
    let cache: Cache = serde_json::from_slice(bytes).map_err(|_| failure())?;
    if cache.schema != 1
        || ![&cache.source, &cache.material, &cache.target]
            .iter()
            .all(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        || !valid_validator(&cache.etag)
        || !valid_validator(&cache.last_modified)
    {
        return Err(failure());
    }
    Ok(cache)
}
fn read(dir: &Path, manager: &Manager, id: &str) -> AppResult<(Option<Cache>, Option<i64>)> {
    let lease = lease(manager)?;
    match storage::get(dir, &lease, &key(id))
        .and_then(|r| r.take(&lease))
        .map_err(|_| failure())?
    {
        None => Ok((None, None)),
        Some(entry) => Ok((Some(decode(&entry.value)?), Some(entry.revision))),
    }
}
fn write(
    dir: &Path,
    manager: &Manager,
    id: &str,
    cache: &Cache,
    expected: Option<i64>,
) -> AppResult<()> {
    let value = serde_json::to_vec(cache).map_err(|_| failure())?;
    let lease = lease(manager)?;
    storage::change(
        dir,
        &lease,
        &[Change::Put {
            key: key(id),
            expected,
            value,
        }],
    )
    .and_then(|r| r.take(&lease))
    .map_err(|_| failure())?;
    Ok(())
}
pub(super) async fn load(manager: Manager, id: String) -> AppResult<(Option<Cache>, Option<i64>)> {
    tauri::async_runtime::spawn_blocking(move || read(&crate::services::data_dir()?, &manager, &id))
        .await
        .map_err(|_| failure())?
}
pub(super) async fn save(
    manager: Manager,
    id: String,
    cache: Cache,
    expected: Option<i64>,
) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        write(
            &crate::services::data_dir()?,
            &manager,
            &id,
            &cache,
            expected,
        )
    })
    .await
    .map_err(|_| failure())?
}
/// Only the check timestamp changes: no material publication, policy selection,
/// override reset, quota replacement, runtime application, or hidden retry.
pub(super) fn checked(
    subscription: &SubscriptionProfile,
    now: u64,
    userinfo: &SubscriptionUserinfo,
) -> SubscriptionProfile {
    let mut result = subscription.clone();
    result.last_sync_at_unix_ms = Some(now);
    result.updated_at_unix_ms = now;
    result.last_error = None;
    // A 304 can carry fresh provider metadata. Missing fields remain last-known,
    // and lastSync is still a content check rather than a traffic sample time.
    if let Some(value) = userinfo.upload {
        result.upload_bytes = Some(value);
    }
    if let Some(value) = userinfo.download {
        result.download_bytes = Some(value);
    }
    if let Some(value) = userinfo.total {
        result.total_bytes = Some(value);
    }
    if let Some(value) = userinfo.expire_ms() {
        result.expire_at_unix_ms = Some(value);
    }
    result
}
#[cfg(test)]
#[path = "conditional_sync/tests.rs"]
mod tests;

/// Caller holds the shared per-ID claim and profile-operation lock. Persistence
/// happens before publishing the in-memory check; injected sink exercises faults.
pub(super) fn confirm_not_modified(
    state: &AppState,
    snapshot: &SubscriptionProfile,
    user_agent: &str,
    cache: &Cache,
    userinfo: &SubscriptionUserinfo,
    now: u64,
    persist: impl FnOnce(&[SubscriptionProfile]) -> AppResult<()>,
) -> AppResult<(SubscriptionProfile, Cache)> {
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let profile = profiles
        .iter()
        .find(|profile| Some(&profile.id) == snapshot.target_proxy_config_id.as_ref())
        .ok_or_else(|| AppError::invalid_argument("304 的关联配置材料不存在，请重新同步"))?;
    let mut subscriptions = lock(state.subscriptions(), "subscription")?;
    let current = subscriptions
        .iter()
        .find(|source| source.id == snapshot.id)
        .ok_or_else(|| AppError::not_found("subscription", snapshot.id.clone()))?;
    if serde_json::to_value(current).ok() != serde_json::to_value(snapshot).ok()
        || !cache.matches(current, user_agent, profile)
    {
        return Err(AppError::conflict(
            "subscription",
            snapshot.id.clone(),
            "304 的来源或配置材料已变化，请重新同步",
        ));
    }
    let updated = checked(current, now, userinfo);
    let refreshed = Cache::confirmed(&updated, user_agent, profile, &cache.validators())?;
    let mut next = subscriptions.clone();
    *next
        .iter_mut()
        .find(|source| source.id == snapshot.id)
        .expect("checked source") = updated.clone();
    persist(&next)?;
    *subscriptions = next;
    Ok((updated, refreshed))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionalCacheReset {
    pub cleared: bool,
}

fn delete_observed(dir: &Path, manager: &Manager, id: &str, revision: i64) -> AppResult<()> {
    let access = lease(manager)?;
    storage::change(
        dir,
        &access,
        &[Change::Delete {
            key: key(id),
            expected: revision,
        }],
    )
    .and_then(|result| result.take(&access))
    .map_err(|error| match error {
        znet_client_core::capability::Error::Busy => AppError::conflict(
            "subscription-cache",
            id,
            "条件请求记录已变化，请刷新后重新确认重置",
        ),
        _ => failure(),
    })?;
    Ok(())
}

// The host selects the private database directory. Unlike load(), reset must
// not decode the row: its intended use includes repairing corrupt metadata.
fn reset_at(
    state: &AppState,
    dir: &Path,
    id: &str,
    expected_updated_at_unix_ms: u64,
) -> AppResult<ConditionalCacheReset> {
    let _claim = begin_in_flight(state.subscription_syncs(), "subscription", id)?;
    let subscriptions = lock(state.subscriptions(), "subscription")?;
    let source = subscriptions
        .iter()
        .find(|source| source.id == id)
        .ok_or_else(|| AppError::not_found("subscription", id))?;
    if source.managed_source.is_some() {
        return Err(AppError::invalid_argument(
            "插件托管订阅不使用普通订阅条件记录，请通过来源管理",
        ));
    }
    if source.updated_at_unix_ms != expected_updated_at_unix_ms {
        return Err(AppError::conflict(
            "subscription",
            id,
            "订阅来源已变化，请刷新后重新确认重置",
        ));
    }
    let access = lease(state.capabilities())?;
    let entry = storage::get(dir, &access, &key(id))
        .and_then(|result| result.take(&access))
        .map_err(|_| failure())?;
    let Some(entry) = entry else {
        return Ok(ConditionalCacheReset { cleared: false });
    };
    delete_observed(dir, state.capabilities(), id, entry.revision)?;
    Ok(ConditionalCacheReset { cleared: true })
}

pub fn reset_conditional_cache(
    state: tauri::State<'_, AppState>,
    id: String,
    expected_updated_at_unix_ms: u64,
) -> AppResult<ConditionalCacheReset> {
    let id = normalize_required(id, "id")?;
    reset_at(
        state.inner(),
        &crate::services::data_dir()?,
        &id,
        expected_updated_at_unix_ms,
    )
}
