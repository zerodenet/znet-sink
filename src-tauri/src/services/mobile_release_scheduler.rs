//! Native foreground-only release checks. Read-only GET retries are not business
//! operation replays. Preference writes have their own durable owned receipt.
use super::{
    app_config_store,
    common::{lock, now_unix_ms},
    file_history::{self, Domain, HistoryToken, WriteReceipt},
    mobile_release, schedule_store,
};
use crate::{
    client_core::ClientScope,
    errors::{AppError, AppResult},
    models::{app_config::AppConfig, mobile_release::*},
    state::app_state::AppState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};

pub(crate) const INTERVAL_MS: u64 = 6 * 60 * 60 * 1000;
pub(crate) const RETRY_MS: u64 = 15 * 60 * 1000;
const STORE: &str = "mobile-releases";
const EVENT: &str = "mobile-release:schedule-updated";
const MAX_OPERATIONS: usize = 8;
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Attempt {
    started: u64,
    success: Option<u64>,
    retry: Option<u64>,
    prefs: String,
    outcome: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PreferenceRecord {
    view: ReleasePreferenceOperation,
    session: String,
    scope: ClientScope,
    generation: u64,
    candidate_digest: String,
    before: HistoryToken,
    receipt: Option<WriteReceipt>,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Checkpoint {
    attempts: BTreeMap<String, Attempt>,
    operations: Vec<PreferenceRecord>,
}
#[derive(Clone)]
struct Binding {
    session: String,
    scope: ClientScope,
    generation: u64,
    config_digest: String,
    prefs_digest: String,
    history: Option<HistoryToken>,
    lifecycle: u64,
}
impl Binding {
    fn same(&self, other: &Self) -> bool {
        self.session == other.session
            && self.scope == other.scope
            && self.generation == other.generation
            && self.config_digest == other.config_digest
            && self.history == other.history
            && self.lifecycle == other.lifecycle
    }
}
#[derive(Clone)]
struct Cache {
    body: Vec<u8>,
    etag: Option<String>,
}
#[derive(Clone)]
struct Published {
    catalog: MobileReleaseCatalog,
    binding: Binding,
}
struct Flight {
    id: String,
    key: String,
    binding: Binding,
}
#[derive(Clone)]
struct Review {
    operation: String,
    binding: Binding,
    store_revision: Option<i64>,
    deadline: Instant,
}
#[derive(Default)]
struct State {
    loaded: bool,
    store_revision: Option<i64>,
    checkpoint: Checkpoint,
    storage_error: bool,
    foreground: bool,
    lifecycle: u64,
    worker: bool,
    revision: u64,
    next_request: u64,
    last_binding: Option<Binding>,
    waiters: usize,
    flight: Option<Flight>,
    completed: VecDeque<(String, AppResult<MobileReleaseCatalog>)>,
    // Two repositories only. Each body has already passed the complete 4 MiB
    // parser; channels are derived without retaining six duplicate bodies.
    caches: BTreeMap<String, Cache>,
    published: BTreeMap<String, Published>,
    reviews: BTreeMap<String, Review>,
}
pub struct MobileReleaseScheduler {
    state: Mutex<State>,
    complete: Condvar,
    wake: Arc<tokio::sync::Notify>,
    #[cfg(test)]
    directory: Mutex<Option<PathBuf>>,
    #[cfg(test)]
    before_file: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}
impl Default for MobileReleaseScheduler {
    fn default() -> Self {
        Self {
            state: Mutex::new(State::default()),
            complete: Condvar::new(),
            wake: Arc::new(tokio::sync::Notify::new()),
            #[cfg(test)]
            directory: Mutex::new(None),
            #[cfg(test)]
            before_file: Mutex::new(None),
        }
    }
}
fn fail(code: &'static str, text: &str) -> AppError {
    AppError {
        code,
        message: text.into(),
        details: None,
    }
}
fn storage_error() -> AppError {
    fail(
        "release_schedule_storage",
        "发行检查记录无法核对，自动检查已暂停；手动目录读取仍可使用",
    )
}
fn unknown_preference() -> AppError {
    fail(
        "release_preferences_unknown",
        "原发行偏好保存或确认结果未确认；请核对原操作，不会自动重放",
    )
}
fn invalidated() -> AppError {
    fail(
        "release_check_invalidated",
        "检查期间页面宿主、配置、偏好或前台状态已变化；原结果仅作历史，不替换当前目录",
    )
}
fn digest<T: Serialize>(value: &T) -> AppResult<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).map_err(|_| storage_error())?)
    ))
}
fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
}
fn key(source: MobileReleaseSource, channel: MobileReleaseChannel) -> String {
    format!(
        "{}:{}",
        repo(source),
        match channel {
            MobileReleaseChannel::Stable => "stable",
            MobileReleaseChannel::Preview => "preview",
            MobileReleaseChannel::Test => "test",
        }
    )
}
fn repo(source: MobileReleaseSource) -> &'static str {
    match source {
        MobileReleaseSource::Client => "client",
        MobileReleaseSource::Kernel => "kernel",
    }
}
fn next_at(attempt: &Attempt) -> u64 {
    if matches!(
        attempt.outcome.as_str(),
        "failed" | "invalidated" | "interrupted"
    ) {
        attempt.retry.unwrap_or(0)
    } else {
        attempt.success.map_or(0, |t| t.saturating_add(INTERVAL_MS))
    }
}
fn unresolved(record: &PreferenceRecord) -> bool {
    !matches!(record.view.state.as_str(), "confirmed" | "rejected")
}
fn operation_view(record: &PreferenceRecord, storage_failed: bool) -> ReleasePreferenceOperation {
    let mut view = record.view.clone();
    view.review_binding = None;
    if storage_failed {
        view.state = "unknown".into();
        view.acknowledged = false;
        view.reason = Some("原偏好确认记录未能持久核对；自动检查暂停，请只读核对原操作".into());
    }
    view
}
fn validate(checkpoint: &Checkpoint) -> bool {
    checkpoint.attempts.len() <= 6
        && checkpoint.operations.len() <= MAX_OPERATIONS
        && checkpoint.attempts.iter().all(|(k, a)| {
            [
                "client:stable",
                "client:preview",
                "client:test",
                "kernel:stable",
                "kernel:preview",
                "kernel:test",
            ]
            .contains(&k.as_str())
                && hex(&a.prefs, 64)
                && [
                    "checking",
                    "success",
                    "failed",
                    "invalidated",
                    "interrupted",
                ]
                .contains(&a.outcome.as_str())
        })
        && checkpoint.operations.iter().all(|r| {
            hex(&r.view.operation_id, 32)
                && hex(&r.session, 32)
                && hex(&r.view.expected_revision, 64)
                && hex(&r.candidate_digest, 64)
                && r.view.reason.as_deref().is_none_or(|reason| {
                    [
                        "本地保存已准备，尚未确认文件结果",
                        "偏好已保存；请确认原本地回执后才启用自动检查",
                        "偏好保存前来源已变化，未写入文件；请重新读取",
                        unknown_preference().message.as_str(),
                    ]
                    .contains(&reason)
                })
                && r.generation > 0
                && [
                    "prepared",
                    "unknown",
                    "saved-awaiting-confirm",
                    "confirmed",
                    "rejected",
                ]
                .contains(&r.view.state.as_str())
                && file_history::validate_token(&r.before).is_ok()
                && (r.view.state != "confirmed" || (r.view.acknowledged && r.receipt.is_some()))
                && (r.view.state != "saved-awaiting-confirm"
                    || (r.receipt.is_some() && !r.view.acknowledged))
                && r.view.review_binding.is_none()
                && (r.view.receipt.is_some() == r.receipt.is_some())
                && r.receipt.as_ref().is_none_or(|receipt| {
                    receipt.before == r.before
                        && receipt.owner_operation.as_deref() == Some(r.view.operation_id.as_str())
                        && receipt.before.epoch.checked_add(1) == Some(receipt.after.epoch)
                        && file_history::validate_token(&receipt.after).is_ok()
                        && receipt.before.store_id == receipt.after.store_id
                        && receipt.before.file_id == receipt.after.file_id
                        && r.view.receipt.as_ref().is_some_and(|v| {
                            v.owner_operation == r.view.operation_id
                                && v.before_epoch == receipt.before.epoch.to_string()
                                && v.after_epoch == receipt.after.epoch.to_string()
                        })
                })
        })
        && checkpoint
            .operations
            .iter()
            .map(|r| &r.view.operation_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == checkpoint.operations.len()
        && checkpoint
            .operations
            .iter()
            .all(|r| r.view.acknowledged == (r.view.state == "confirmed"))
        && checkpoint
            .operations
            .iter()
            .filter(|r| unresolved(r))
            .count()
            <= 1
}
impl MobileReleaseScheduler {
    fn dir(&self) -> AppResult<PathBuf> {
        #[cfg(test)]
        if let Some(dir) = lock(&self.directory, "release_directory")?.clone() {
            return Ok(dir);
        }
        super::data_dir()
    }
    fn initialize(&self, state: &AppState, inner: &mut State) -> AppResult<()> {
        if inner.loaded {
            return if inner.storage_error {
                Err(storage_error())
            } else {
                Ok(())
            };
        }
        let loaded =
            schedule_store::load_at::<Checkpoint>(&self.dir()?, state.capabilities(), STORE);
        inner.loaded = true;
        match loaded {
            Ok((mut checkpoint, revision)) if validate(&checkpoint) => {
                for attempt in checkpoint.attempts.values_mut() {
                    if attempt.outcome == "checking" {
                        attempt.outcome = "interrupted".into();
                        attempt.retry = Some(attempt.started.saturating_add(RETRY_MS));
                    }
                }
                inner.checkpoint = checkpoint;
                inner.store_revision = revision;
                Ok(())
            }
            _ => {
                inner.storage_error = true;
                Err(storage_error())
            }
        }
    }
    fn persist(&self, state: &AppState, inner: &mut State) -> AppResult<()> {
        if inner.storage_error {
            return Err(storage_error());
        }
        if !validate(&inner.checkpoint) {
            inner.storage_error = true;
            return Err(storage_error());
        }
        if schedule_store::save_at(
            &self.dir()?,
            state.capabilities(),
            STORE,
            &inner.checkpoint,
            &mut inner.store_revision,
        )
        .is_err()
        {
            inner.storage_error = true;
            return Err(storage_error());
        }
        inner.revision = inner.revision.saturating_add(1);
        Ok(())
    }
    fn context(&self, state: &AppState) -> AppResult<(AppConfig, Binding)> {
        let config = lock(state.app_config(), "app_config")?.clone();
        let config_digest = mobile_release::revision(&config)?;
        // Only automatic checks need a clean saved preference. An uncertain
        // file does not prevent an explicitly requested read-only catalog GET.
        let history = self.dir().ok().and_then(|dir| {
            let guard =
                file_history::acquire(&dir.join("app-config.json"), Domain::AppConfig).ok()?;
            let token = guard.token().ok()?;
            (app_config_store::read_current_locked(&guard).ok()? == config).then_some(token)
        });
        let core = state.client_core_snapshot();
        let lifecycle = lock(&self.state, "release_scheduler")?.lifecycle;
        Ok((
            config.clone(),
            Binding {
                session: state.host_session_id()?.into(),
                scope: core.scope,
                generation: core.scope_generation.0,
                config_digest,
                prefs_digest: digest(&config.release_preferences)?,
                history,
                lifecycle,
            },
        ))
    }
    pub(crate) fn snapshot(&self, state: &AppState) -> AppResult<ReleaseScheduleSnapshot> {
        let (config, binding) = self.context(state)?;
        let mut inner = lock(&self.state, "release_scheduler")?;
        let _ = self.initialize(state, &mut inner);
        if inner
            .last_binding
            .as_ref()
            .is_none_or(|b| !b.same(&binding))
        {
            inner.revision = inner.revision.saturating_add(1);
            inner.last_binding = Some(binding.clone());
        }
        let target = key(
            MobileReleaseSource::Client,
            config.release_preferences.client_channel,
        );
        let attempt = inner.checkpoint.attempts.get(&target);
        let operation = inner
            .checkpoint
            .operations
            .last()
            .map(|r| operation_view(r, inner.storage_error));
        let blocked = inner.checkpoint.operations.iter().any(unresolved);
        let due = attempt.map(next_at);
        let cached = inner.published.get(&target);
        let cache_current = cached.is_some_and(|p| p.binding.same(&binding))
            && !inner.storage_error
            && attempt.is_some_and(|a| a.outcome == "success");
        Ok(ReleaseScheduleSnapshot {
            host_session_id: binding.session.clone(),
            revision: inner.revision.to_string(),
            foreground: inner.foreground,
            checking: inner.flight.is_some(),
            enabled: config.release_preferences.client_auto_check,
            client_channel: config.release_preferences.client_channel,
            last_attempted_at_unix_ms: attempt.map(|a| a.started.to_string()),
            last_success_at_unix_ms: attempt.and_then(|a| a.success).map(|t| t.to_string()),
            next_due_at_unix_ms: due.map(|t| t.to_string()),
            retry_at_unix_ms: attempt.and_then(|a| a.retry).map(|t| t.to_string()),
            outcome: if inner.storage_error {
                "storage-error"
            } else if blocked || binding.history.is_none() {
                "preferences-unconfirmed"
            } else if inner.flight.is_some() {
                "checking"
            } else {
                attempt.map_or("never", |a| a.outcome.as_str())
            }
            .into(),
            reason: if inner.storage_error {
                Some(storage_error().message)
            } else if blocked || binding.history.is_none() {
                Some(unknown_preference().message)
            } else {
                None
            },
            storage_error: inner.storage_error,
            cached_catalog: cached.map(|p| p.catalog.clone()),
            cache_current,
            preference_operation: operation,
            foreground_scheduling: if cfg!(any(target_os = "android", target_os = "ios")) {
                "native-lifecycle"
            } else {
                "not-native-host"
            },
            background_scheduling: "not-adapted",
        })
    }
    fn should_check(inner: &State, config: &AppConfig, binding: &Binding, now: u64) -> bool {
        if !inner.foreground
            || inner.storage_error
            || binding.history.is_none()
            || !config.release_preferences.client_auto_check
            || inner.checkpoint.operations.iter().any(unresolved)
        {
            return false;
        }
        let k = key(
            MobileReleaseSource::Client,
            config.release_preferences.client_channel,
        );
        inner
            .checkpoint
            .attempts
            .get(&k)
            .is_none_or(|a| a.prefs != binding.prefs_digest || now >= next_at(a))
    }
    pub(crate) fn check(
        &self,
        state: &AppState,
        source: MobileReleaseSource,
        channel: MobileReleaseChannel,
        automatic: bool,
    ) -> AppResult<MobileReleaseCatalog> {
        self.check_with(state, source, channel, automatic, |etag| {
            mobile_release::fetch(state.capabilities(), source, etag)
        })
    }
    fn check_with(
        &self,
        state: &AppState,
        source: MobileReleaseSource,
        channel: MobileReleaseChannel,
        automatic: bool,
        fetch: impl FnOnce(Option<&str>) -> AppResult<mobile_release::Reply>,
    ) -> AppResult<MobileReleaseCatalog> {
        let (config, binding) = self.context(state)?;
        let request_key = key(source, channel);
        let mut inner = lock(&self.state, "release_scheduler")?;
        let _ = self.initialize(state, &mut inner);
        loop {
            if let Some(flight) = inner.flight.as_ref() {
                let wait_id = flight.id.clone();
                let shared = flight.key == request_key && flight.binding.same(&binding);
                inner.waiters = inner.waiters.saturating_add(1);
                self.complete.notify_all();
                inner = self.complete.wait(inner).map_err(|_| storage_error())?;
                inner.waiters = inner.waiters.saturating_sub(1);
                if shared {
                    if let Some((_, result)) = inner.completed.iter().find(|(id, _)| id == &wait_id)
                    {
                        return result.clone();
                    }
                }
                continue;
            }
            break;
        }
        if !inner.foreground && cfg!(any(target_os = "android", target_os = "ios")) {
            return Err(fail(
                "release_background",
                "应用已在后台，未发起新的发行检查",
            ));
        }
        if automatic
            && (source != MobileReleaseSource::Client
                || channel != config.release_preferences.client_channel
                || !Self::should_check(&inner, &config, &binding, now_unix_ms()))
        {
            return Err(fail("release_not_due", "当前没有到期的客户端发行检查"));
        }
        if inner.lifecycle != binding.lifecycle {
            return Err(invalidated());
        }
        inner.next_request = inner.next_request.saturating_add(1);
        let request_id = format!("{}-{}", binding.session, inner.next_request);
        let started = now_unix_ms();
        let previous = inner
            .checkpoint
            .attempts
            .get(&request_key)
            .cloned()
            .unwrap_or_default();
        inner.checkpoint.attempts.insert(
            request_key.clone(),
            Attempt {
                started,
                success: previous.success,
                retry: previous.retry,
                prefs: binding.prefs_digest.clone(),
                outcome: "checking".into(),
            },
        );
        let persisted = self.persist(state, &mut inner);
        if automatic {
            persisted?;
        }
        let cached = inner.caches.get(repo(source)).cloned();
        inner.flight = Some(Flight {
            id: request_id.clone(),
            key: request_key.clone(),
            binding: binding.clone(),
        });
        drop(inner);
        let dispatch = self
            .context(state)
            .is_ok_and(|(_, current)| current.same(&binding));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if dispatch {
                fetch(cached.as_ref().and_then(|c| c.etag.as_deref()))
            } else {
                Err(invalidated())
            }
        }))
        .unwrap_or_else(|_| Err(fail("release_transport", "发行目录请求未完成，请重新读取")))
        .and_then(|reply| {
            let body = match reply.status {
                200 => reply.body,
                304 => cached
                    .as_ref()
                    .filter(|c| c.etag.is_some())
                    .map(|c| c.body.clone())
                    .ok_or_else(|| {
                        fail(
                            "release_cache_missing",
                            "目录返回未修改，但原已验证缓存不可用；未将其视为空目录",
                        )
                    })?,
                _ => {
                    return Err(fail(
                        "release_upstream",
                        "发行目录未返回有效结果，请稍后重试",
                    ))
                }
            };
            let catalog = mobile_release::build_catalog(
                &body,
                source,
                channel,
                request_id.clone(),
                reply.status,
                now_unix_ms(),
            )?;
            let etag = reply.etag.filter(|e| {
                !e.trim().is_empty() && e.len() <= 512 && !e.chars().any(char::is_control)
            });
            Ok((
                catalog,
                Cache {
                    body,
                    etag: if reply.status == 304 {
                        etag.or_else(|| cached.as_ref().and_then(|c| c.etag.clone()))
                    } else {
                        etag
                    },
                },
            ))
        });
        let current = self.context(state).ok().map(|(_, b)| b);
        let mut inner = lock(&self.state, "release_scheduler")?;
        let current_result = if (!inner.foreground
            && cfg!(any(target_os = "android", target_os = "ios")))
            || current.as_ref().is_none_or(|b| !b.same(&binding))
        {
            Err(invalidated())
        } else {
            result
                .as_ref()
                .map(|(c, _)| c.clone())
                .map_err(Clone::clone)
        };
        let now = now_unix_ms();
        if let Ok((catalog, cache)) = &result {
            // A verified public body remains useful as a conditional cache even
            // when its delivery was retired; it grants no preference authority.
            inner.caches.insert(repo(source).into(), cache.clone());
            if current_result.is_ok() {
                inner.published.insert(
                    request_key.clone(),
                    Published {
                        catalog: catalog.clone(),
                        binding: binding.clone(),
                    },
                );
            }
        }
        if let Some(attempt) = inner.checkpoint.attempts.get_mut(&request_key) {
            match &current_result {
                Ok(_) => {
                    attempt.success = Some(now);
                    attempt.retry = None;
                    attempt.outcome = "success".into();
                }
                Err(e) => {
                    if e.code == "release_check_invalidated" {
                        attempt.retry = None;
                        attempt.success = previous.success;
                        attempt.outcome = "invalidated".into();
                    } else {
                        attempt.retry = Some(now.saturating_add(RETRY_MS));
                        attempt.outcome = "failed".into();
                    }
                }
            }
        }
        let _ = self.persist(state, &mut inner);
        inner.flight = None;
        inner
            .completed
            .push_back((request_id, current_result.clone()));
        while inner.completed.len() > 8 {
            inner.completed.pop_front();
        }
        self.complete.notify_all();
        self.wake.notify_one();
        current_result
    }
    pub(crate) fn preferences(&self, state: &AppState) -> AppResult<ReleasePreferencesSnapshot> {
        let config = lock(state.app_config(), "app_config")?.clone();
        let mut inner = lock(&self.state, "release_scheduler")?;
        let _ = self.initialize(state, &mut inner);
        let mut result = mobile_release::preferences(&config)?;
        result.operation = inner
            .checkpoint
            .operations
            .last()
            .map(|r| operation_view(r, inner.storage_error));
        if inner.storage_error {
            result.scheduler_state = "storage-error";
        } else if inner.checkpoint.operations.iter().any(unresolved) {
            result.scheduler_state = "preferences-unconfirmed";
        }
        Ok(result)
    }
    pub(crate) fn operation(
        &self,
        state: &AppState,
        id: &str,
    ) -> AppResult<Option<ReleasePreferenceOperation>> {
        if !hex(id, 32) {
            return Err(fail("invalid_argument", "原发行偏好操作身份无效"));
        }
        let (config, binding) = self.context(state)?;
        let mut inner = lock(&self.state, "release_scheduler")?;
        self.initialize(state, &mut inner)?;
        let Some(record) = inner
            .checkpoint
            .operations
            .iter()
            .find(|r| r.view.operation_id == id)
            .cloned()
        else {
            return Ok(None);
        };
        let expected_store_revision = inner.store_revision;
        drop(inner);
        let mut view = record.view.clone();
        view.review_binding = None;
        let owned = self
            .dir()
            .ok()
            .and_then(|dir| {
                file_history::acquire(&dir.join("app-config.json"), Domain::AppConfig).ok()
            })
            .and_then(|history| history.owned_token(id, &record.before).ok());
        // Never acquire a file-history lock under the scheduler mutex:
        // confirmation owns app -> file -> core -> scheduler in that order.
        let mut inner = lock(&self.state, "release_scheduler")?;
        self.initialize(state, &mut inner)?;
        if inner.store_revision != expected_store_revision {
            return Err(invalidated());
        }
        if record.receipt.as_ref().is_some_and(|r| {
            Some(&r.after) == binding.history.as_ref() && Some(&r.after) == owned.as_ref()
        }) && binding.config_digest == record.candidate_digest
            && config.release_preferences == record.view.preferences
            && matches!(view.state.as_str(), "saved-awaiting-confirm" | "confirmed")
        {
            inner.next_request = inner.next_request.saturating_add(1);
            let token = digest(&(
                "release-preference-review-v1",
                &binding.session,
                &binding.scope,
                binding.generation,
                &binding.config_digest,
                &binding.history,
                id,
                inner.store_revision,
                inner.next_request,
            ))?;
            inner
                .reviews
                .retain(|_, r| r.operation != id && Instant::now() < r.deadline);
            while inner.reviews.len() >= MAX_OPERATIONS {
                let Some(key) = inner.reviews.keys().next().cloned() else {
                    break;
                };
                inner.reviews.remove(&key);
            }
            let store_revision = inner.store_revision;
            inner.reviews.insert(
                token.clone(),
                Review {
                    operation: id.into(),
                    binding,
                    store_revision,
                    deadline: Instant::now() + Duration::from_secs(300),
                },
            );
            view.review_binding = Some(token);
        }
        Ok(Some(view))
    }
    pub(crate) fn save_preferences(
        &self,
        state: &AppState,
        id: &str,
        expected: &str,
        prefs: ReleasePreferences,
    ) -> AppResult<ReleasePreferencesSnapshot> {
        if !hex(id, 32) {
            return Err(fail(
                "invalid_argument",
                "保存前必须固定有效的发行偏好操作身份",
            ));
        }
        let (current, binding) = self.context(state)?;
        let candidate = mobile_release::candidate(&current, expected, prefs.clone())?;
        let before = binding.history.clone().ok_or_else(unknown_preference)?;
        let candidate_digest = mobile_release::revision(&candidate)?;
        {
            let mut inner = lock(&self.state, "release_scheduler")?;
            self.initialize(state, &mut inner)?;
            if inner
                .checkpoint
                .operations
                .iter()
                .any(|r| r.view.operation_id == id)
            {
                return Err(fail(
                    "release_preferences_original_operation",
                    "原偏好操作已记录；请只读核对原结果，不会再次保存",
                ));
            }
            if inner.checkpoint.operations.iter().any(unresolved) {
                return Err(unknown_preference());
            }
            while inner.checkpoint.operations.len() >= MAX_OPERATIONS {
                inner.checkpoint.operations.remove(0);
            }
            inner.checkpoint.operations.push(PreferenceRecord {
                view: ReleasePreferenceOperation {
                    operation_id: id.into(),
                    expected_revision: expected.into(),
                    preferences: prefs,
                    state: "prepared".into(),
                    acknowledged: false,
                    reason: Some("本地保存已准备，尚未确认文件结果".into()),
                    receipt: None,
                    review_binding: None,
                },
                session: binding.session.clone(),
                scope: binding.scope.clone(),
                generation: binding.generation,
                candidate_digest,
                before: before.clone(),
                receipt: None,
            });
            self.persist(state, &mut inner)?;
        }
        self.wake.notify_one();
        // No owning scheduler mutex is held while taking AppState/file guards.
        #[cfg(test)]
        if let Some(action) = self.before_file.lock().unwrap().take() {
            action();
        }
        let mut attempted_write = false;
        let write = (|| {
            let mut actual = lock(state.app_config(), "app_config")?;
            if *actual != current {
                return Err(invalidated());
            }
            let mut history =
                file_history::acquire(&self.dir()?.join("app-config.json"), Domain::AppConfig)?;
            history.require(&before)?;
            if app_config_store::read_current_locked(&history)? != current {
                return Err(invalidated());
            }
            state.with_client_scope_generation(&binding.scope, binding.generation, || {
                if state.host_session_id()? != binding.session {
                    return Err(invalidated());
                }
                attempted_write = true;
                let receipt = app_config_store::save_owned_locked(&mut history, &candidate, id)?;
                *actual = candidate.clone();
                Ok(receipt)
            })
        })();
        let mut inner = lock(&self.state, "release_scheduler")?;
        let record = inner
            .checkpoint
            .operations
            .iter_mut()
            .find(|r| r.view.operation_id == id)
            .ok_or_else(unknown_preference)?;
        match write {
            Ok(receipt) => {
                record.view.state = "saved-awaiting-confirm".into();
                record.view.reason = Some("偏好已保存；请确认原本地回执后才启用自动检查".into());
                record.view.receipt = Some(ReleasePreferenceReceipt {
                    owner_operation: id.into(),
                    before_epoch: receipt.before.epoch.to_string(),
                    after_epoch: receipt.after.epoch.to_string(),
                });
                record.receipt = Some(receipt);
            }
            Err(_) => {
                record.view.state = if attempted_write {
                    "unknown"
                } else {
                    "rejected"
                }
                .into();
                record.view.reason = Some(if attempted_write {
                    unknown_preference().message
                } else {
                    "偏好保存前来源已变化，未写入文件；请重新读取".into()
                });
                let _ = self.persist(state, &mut inner);
                self.wake.notify_one();
                return Err(if attempted_write {
                    unknown_preference()
                } else {
                    fail(
                        "release_preferences_rejected",
                        "偏好保存前来源已变化，未写入文件；请重新读取",
                    )
                });
            }
        }
        self.persist(state, &mut inner)
            .map_err(|_| unknown_preference())?;
        let operation = inner
            .checkpoint
            .operations
            .iter()
            .find(|r| r.view.operation_id == id)
            .map(|r| r.view.clone());
        drop(inner);
        let mut result = mobile_release::preferences(&candidate)?;
        result.operation = operation;
        result.scheduler_state = "preferences-unconfirmed";
        result.operation = self.operation(state, id)?;
        self.wake.notify_one();
        Ok(result)
    }
    pub(crate) fn confirm_preferences(
        &self,
        state: &AppState,
        id: &str,
        review_binding: &str,
    ) -> AppResult<ReleasePreferencesSnapshot> {
        if !hex(id, 32) || !hex(review_binding, 64) {
            return Err(fail("invalid_argument", "原发行偏好核对身份无效"));
        }
        let (record, review) = {
            let mut inner = lock(&self.state, "release_scheduler")?;
            self.initialize(state, &mut inner)?;
            let review = inner
                .reviews
                .get(review_binding)
                .filter(|r| {
                    r.operation == id
                        && Instant::now() < r.deadline
                        && r.store_revision == inner.store_revision
                })
                .cloned()
                .ok_or_else(invalidated)?;
            let record = inner
                .checkpoint
                .operations
                .iter()
                .find(|r| r.view.operation_id == id)
                .cloned()
                .ok_or_else(unknown_preference)?;
            (record, review)
        };
        let receipt = record
            .receipt
            .as_ref()
            .filter(|_| {
                matches!(
                    record.view.state.as_str(),
                    "saved-awaiting-confirm" | "confirmed"
                )
            })
            .ok_or_else(unknown_preference)?;
        let actual = lock(state.app_config(), "app_config")?;
        if mobile_release::revision(&actual)? != review.binding.config_digest
            || mobile_release::revision(&actual)? != record.candidate_digest
            || actual.release_preferences != record.view.preferences
        {
            return Err(invalidated());
        }
        let history =
            file_history::acquire(&self.dir()?.join("app-config.json"), Domain::AppConfig)?;
        history.require(&receipt.after)?;
        if history.owned_token(id, &record.before)? != receipt.after {
            return Err(invalidated());
        }
        if Some(&history.token()?) != review.binding.history.as_ref()
            || app_config_store::read_current_locked(&history)? != *actual
        {
            return Err(invalidated());
        }
        state.with_client_scope_generation(
            &review.binding.scope,
            review.binding.generation,
            || {
                if state.host_session_id()? != review.binding.session {
                    return Err(invalidated());
                }
                let mut inner = lock(&self.state, "release_scheduler")?;
                if inner.store_revision != review.store_revision
                    || inner.lifecycle != review.binding.lifecycle
                {
                    return Err(invalidated());
                }
                let target = inner
                    .checkpoint
                    .operations
                    .iter_mut()
                    .find(|r| r.view.operation_id == id)
                    .ok_or_else(unknown_preference)?;
                if target.view.state == "confirmed" && target.view.acknowledged {
                    return Ok(());
                }
                if target.view.state != "saved-awaiting-confirm" {
                    return Err(unknown_preference());
                }
                target.view.state = "confirmed".into();
                target.view.acknowledged = true;
                target.view.reason = None;
                self.persist(state, &mut inner)?;
                inner.reviews.remove(review_binding);
                Ok(())
            },
        )?;
        // Drop metadata guards before the public snapshot reacquires AppState.
        drop(history);
        drop(actual);
        self.wake.notify_one();
        self.preferences(state)
    }
    fn enter(&self) -> AppResult<bool> {
        let mut inner = lock(&self.state, "release_scheduler")?;
        if !inner.foreground {
            inner.foreground = true;
            inner.lifecycle = inner.lifecycle.saturating_add(1);
            inner.revision = inner.revision.saturating_add(1);
        }
        let start = !inner.worker;
        inner.worker = true;
        self.wake.notify_one();
        Ok(start)
    }
    fn leave(&self) {
        if let Ok(mut inner) = self.state.lock() {
            if inner.foreground {
                inner.foreground = false;
                inner.lifecycle = inner.lifecycle.saturating_add(1);
                inner.revision = inner.revision.saturating_add(1);
            }
        }
        self.wake.notify_one();
    }
    fn delay(&self, state: &AppState) -> AppResult<Option<Duration>> {
        let (config, binding) = self.context(state)?;
        let mut inner = lock(&self.state, "release_scheduler")?;
        self.initialize(state, &mut inner)?;
        if !inner.foreground {
            return Ok(None);
        }
        if !config.release_preferences.client_auto_check
            || binding.history.is_none()
            || inner.checkpoint.operations.iter().any(unresolved)
        {
            return Ok(Some(Duration::from_secs(3600)));
        }
        if Self::should_check(&inner, &config, &binding, now_unix_ms()) {
            return Ok(Some(Duration::ZERO));
        }
        let next = inner
            .checkpoint
            .attempts
            .get(&key(
                MobileReleaseSource::Client,
                config.release_preferences.client_channel,
            ))
            .map_or(now_unix_ms(), |a| next_at(a));
        Ok(Some(Duration::from_millis(
            next.saturating_sub(now_unix_ms()).min(INTERVAL_MS),
        )))
    }
}
pub(crate) fn resume(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let owner = state.mobile_release_scheduler();
    if owner.enter().ok() != Some(true) {
        return;
    }
    let _ = app.emit(EVENT, ());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let Some(state) = app.try_state::<AppState>() else {
                break;
            };
            let owner = state.mobile_release_scheduler();
            let wake = owner.wake.clone();
            // Register before computing a deadline so concurrent preference or
            // lifecycle changes cannot disappear between computation and wait.
            let notified = wake.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let due_app = app.clone();
            let delay = tauri::async_runtime::spawn_blocking(move || {
                let state = due_app.state::<AppState>();
                state.mobile_release_scheduler().delay(state.inner())
            })
            .await
            .unwrap_or_else(|_| Err(storage_error()));
            match delay {
                Ok(None) => {
                    let mut inner = match owner.state.lock() {
                        Ok(i) => i,
                        Err(_) => break,
                    };
                    if !inner.foreground {
                        inner.worker = false;
                        break;
                    } else {
                        continue;
                    }
                }
                Ok(Some(delay)) if delay.is_zero() => {
                    let app = app.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        let state = app.state::<AppState>();
                        let channel = lock(state.app_config(), "app_config")
                            .map(|c| c.release_preferences.client_channel);
                        if let Ok(channel) = channel {
                            let _ = state.mobile_release_scheduler().check(
                                state.inner(),
                                MobileReleaseSource::Client,
                                channel,
                                true,
                            );
                        }
                        let _ = app.emit(EVENT, ());
                    })
                    .await;
                }
                Ok(Some(delay)) => {
                    tokio::select! {_=tokio::time::sleep(delay)=>{},_=notified=>{}}
                }
                Err(_) => {
                    tokio::select! {_=tokio::time::sleep(Duration::from_secs(3600))=>{},_=notified=>{}}
                }
            }
        }
    });
}
pub(crate) fn suspend(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.mobile_release_scheduler().leave();
        let _ = app.emit(EVENT, ());
    }
}
#[cfg(test)]
#[path = "mobile_release_scheduler/tests.rs"]
mod tests;
