//! Native foreground due coordination. No UI timer or OS background worker.
//! A durable in-progress checkpoint is an uncertainty barrier after process loss.
use super::{
    common::{lock, now_unix_ms},
    schedule_store, subscription,
};
use crate::{
    errors::AppResult, models::subscription::SubscriptionProfile, state::app_state::AppState,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
const MAX_TARGETS: usize = 8;
const MAX_RECORDS: usize = 64;
const PASS_SECONDS: u64 = 45;
const STORE: &str = "mobile-subscriptions";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AttemptStatus {
    InProgress,
    ResultUnknown,
    Confirmed,
    Failed,
    Skipped,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttemptRecord {
    pub subscription_id: String,
    pub status: AttemptStatus,
    pub started_at_unix_ms: u64,
    pub finished_at_unix_ms: Option<u64>,
    pub baseline_last_sync_at_unix_ms: Option<u64>,
    pub confirmed_last_sync_at_unix_ms: Option<u64>,
    pub next_eligible_at_unix_ms: u64,
    pub failures: u32,
    /// Fixed host code only; never a source URL/header/body/name or exception.
    pub reason_code: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Checkpoint {
    records: BTreeMap<String, AttemptRecord>,
}
#[derive(Default)]
struct State {
    foreground: bool,
    checking: bool,
    generation: u64,
    checkpoint: Checkpoint,
    storage_error: bool,
    last_pass_started_at_unix_ms: Option<u64>,
    last_pass_finished_at_unix_ms: Option<u64>,
    deferred: usize,
}
#[derive(Default)]
pub struct MobileSubscriptionScheduler {
    state: Mutex<State>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileScheduleSnapshot {
    pub foreground: bool,
    pub checking: bool,
    pub generation: String,
    pub foreground_reconciliation: &'static str,
    pub background_scheduling: &'static str,
    pub storage_error: bool,
    pub last_pass_started_at_unix_ms: Option<u64>,
    pub last_pass_finished_at_unix_ms: Option<u64>,
    pub deferred: usize,
    pub attempts: Vec<AttemptRecord>,
}
impl MobileSubscriptionScheduler {
    pub fn snapshot(&self) -> AppResult<MobileScheduleSnapshot> {
        let state = lock(&self.state, "mobile_subscription_scheduler")?;
        Ok(MobileScheduleSnapshot {
            foreground: state.foreground,
            checking: state.checking,
            generation: state.generation.to_string(),
            foreground_reconciliation: if cfg!(any(target_os = "android", target_os = "ios")) {
                "native-lifecycle"
            } else {
                "not-native-host"
            },
            background_scheduling: "not-adapted",
            storage_error: state.storage_error,
            last_pass_started_at_unix_ms: state.last_pass_started_at_unix_ms,
            last_pass_finished_at_unix_ms: state.last_pass_finished_at_unix_ms,
            deferred: state.deferred,
            attempts: state.checkpoint.records.values().cloned().collect(),
        })
    }
    fn enter(&self) -> AppResult<Option<u64>> {
        let mut state = lock(&self.state, "mobile_subscription_scheduler")?;
        if !state.foreground {
            state.foreground = true;
            state.generation = state.generation.saturating_add(1);
        }
        if state.checking {
            return Ok(None);
        }
        state.checking = true;
        state.last_pass_started_at_unix_ms = Some(now_unix_ms());
        Ok(Some(state.generation))
    }
    fn leave(&self) -> AppResult<()> {
        let mut state = lock(&self.state, "mobile_subscription_scheduler")?;
        if state.foreground {
            state.foreground = false;
            state.generation = state.generation.saturating_add(1);
        }
        Ok(())
    }
    fn current(&self, epoch: u64) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| state.foreground && state.generation == epoch)
    }
    fn finish(&self, epoch: u64) -> Option<u64> {
        let mut state = self.state.lock().ok()?;
        state.last_pass_finished_at_unix_ms = Some(now_unix_ms());
        if state.foreground && state.generation != epoch {
            Some(state.generation)
        } else {
            state.checking = false;
            None
        }
    }
    fn publish(&self, checkpoint: &Checkpoint, error: bool, deferred: usize) {
        if let Ok(mut state) = self.state.lock() {
            state.checkpoint = checkpoint.clone();
            state.storage_error = error;
            state.deferred = deferred;
        }
    }
}
pub fn resume(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Ok(Some(epoch)) = state.mobile_subscription_scheduler().enter() else {
        return;
    };
    let _ = app.emit("subscriptions:schedule-updated", ());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut epoch = epoch;
        loop {
            run_pass(&app, epoch).await;
            let Some(state) = app.try_state::<AppState>() else {
                break;
            };
            let next = state.mobile_subscription_scheduler().finish(epoch);
            let _ = app.emit("subscriptions:schedule-updated", ());
            match next {
                Some(next) => epoch = next,
                None => break,
            }
        }
    });
}
pub fn suspend(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        let _ = state.mobile_subscription_scheduler().leave();
        let _ = app.emit("subscriptions:schedule-updated", ());
    }
}
fn storage_failure(app: &AppHandle, checkpoint: &Checkpoint) {
    if let Some(state) = app.try_state::<AppState>() {
        state
            .mobile_subscription_scheduler()
            .publish(checkpoint, true, 0);
    }
    let _ = app.emit("subscriptions:schedule-updated", ());
}
fn storage_failure_existing(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut view) = state.mobile_subscription_scheduler().state.lock() {
            view.storage_error = true;
        }
    }
    let _ = app.emit("subscriptions:schedule-updated", ());
}
async fn run_pass(app: &AppHandle, epoch: u64) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let scheduler = state.mobile_subscription_scheduler();
    let manager = state.capabilities().clone();
    let Ok((mut checkpoint, mut revision)) =
        schedule_store::load::<Checkpoint>(manager.clone(), STORE).await
    else {
        storage_failure_existing(app);
        return;
    };
    if !valid_checkpoint(&checkpoint) {
        storage_failure_existing(app);
        return;
    }
    let profiles = match subscription::scheduled_profiles(state.inner()) {
        Ok(items) => items,
        Err(_) => {
            storage_failure(app, &checkpoint);
            return;
        }
    };
    reconcile(&mut checkpoint, &profiles);
    let now = now_unix_ms();
    let targets = select_due(&profiles, &checkpoint, now);
    let mut deferred = targets.len();
    if let Ok(mut view) = scheduler.state.lock() {
        view.last_pass_started_at_unix_ms = Some(now);
    }
    scheduler.publish(&checkpoint, false, deferred);
    let deadline = Instant::now() + Duration::from_secs(PASS_SECONDS);
    for target in targets.into_iter().take(MAX_TARGETS) {
        if !scheduler.current(epoch) || Instant::now() >= deadline {
            break;
        }
        if super::common::is_in_flight(state.subscription_syncs(), &target.id).unwrap_or(true) {
            continue;
        }
        if !prune(&mut checkpoint, &target.id) {
            continue;
        }
        let failures = checkpoint.records.get(&target.id).map_or(0, |record| {
            if record.failures >= 4 {
                0
            } else {
                record.failures
            }
        });
        checkpoint.records.insert(
            target.id.clone(),
            AttemptRecord {
                subscription_id: target.id.clone(),
                status: AttemptStatus::InProgress,
                started_at_unix_ms: now_unix_ms(),
                finished_at_unix_ms: None,
                baseline_last_sync_at_unix_ms: target.last_sync_at_unix_ms,
                confirmed_last_sync_at_unix_ms: None,
                next_eligible_at_unix_ms: 0,
                failures,
                reason_code: None,
            },
        );
        // Persist intent before the workflow; failure cannot start an unrecorded request.
        if schedule_store::save(manager.clone(), STORE, &checkpoint, &mut revision)
            .await
            .is_err()
        {
            if let Some(record) = checkpoint.records.get_mut(&target.id) {
                record.status = AttemptStatus::Skipped;
                record.reason_code = Some("checkpoint-not-saved".into());
                record.finished_at_unix_ms = Some(now_unix_ms());
            }
            storage_failure(app, &checkpoint);
            return;
        }
        scheduler.publish(&checkpoint, false, deferred);
        if !scheduler.current(epoch) || Instant::now() >= deadline {
            let record = checkpoint
                .records
                .get_mut(&target.id)
                .expect("claimed record");
            record.status = AttemptStatus::Skipped;
            record.reason_code = Some("foreground-ended-before-start".into());
            record.finished_at_unix_ms = Some(now_unix_ms());
            break;
        }
        deferred = deferred.saturating_sub(1);
        let result = subscription::sync_due(app.clone(), target.clone()).await;
        let finished = now_unix_ms();
        let record = checkpoint
            .records
            .get_mut(&target.id)
            .expect("claimed record");
        record.finished_at_unix_ms = Some(finished);
        match result {
            Ok(Some(profile)) => {
                record.status = AttemptStatus::Confirmed;
                record.confirmed_last_sync_at_unix_ms = profile.last_sync_at_unix_ms;
                record.failures = 0;
                record.next_eligible_at_unix_ms = 0;
            }
            Ok(None) => {
                record.status = AttemptStatus::Skipped;
                record.reason_code = Some("target-changed-or-busy".into());
                record.next_eligible_at_unix_ms = finished.saturating_add(60_000);
            }
            Err(error)
                if !matches!(
                    error.code,
                    "upstream_error" | "invalid_argument" | "not_found" | "conflict"
                ) =>
            {
                record.status = AttemptStatus::ResultUnknown;
                record.reason_code = Some("check-original-result".into());
            }
            Err(error) => {
                record.status = AttemptStatus::Failed;
                record.failures = record.failures.saturating_add(1);
                record.reason_code = Some(safe_reason(error.code).into());
                record.next_eligible_at_unix_ms = finished.saturating_add(retry_delay_ms(
                    record.failures,
                    target.update_interval_secs.unwrap_or(60),
                ));
            }
        }
        if schedule_store::save(manager.clone(), STORE, &checkpoint, &mut revision)
            .await
            .is_err()
        {
            record_unknown(&mut checkpoint, &target.id);
            storage_failure(app, &checkpoint);
            return;
        }
        scheduler.publish(&checkpoint, false, deferred);
        let _ = app.emit(
            "subscriptions:updated",
            serde_json::json!({"subscriptionId":target.id}),
        );
        let _ = app.emit("subscriptions:schedule-updated", ());
    }
    // Persist recovery transitions even if no target executes on this foreground event.
    if schedule_store::save(manager, STORE, &checkpoint, &mut revision)
        .await
        .is_err()
    {
        storage_failure(app, &checkpoint);
    } else {
        scheduler.publish(&checkpoint, false, deferred);
    }
}
pub(crate) fn safe_reason(code: &str) -> &'static str {
    match code {
        "upstream_error" => "upstream-error",
        "invalid_argument" => "invalid-source",
        "conflict" => "runtime-or-resource-conflict",
        "not_found" => "target-removed",
        _ => "synchronization-failed",
    }
}
fn retry_delay_ms(failures: u32, interval: u64) -> u64 {
    if failures >= 4 {
        return interval.max(900).saturating_mul(1000);
    }
    60_000u64.saturating_mul(1u64 << failures.saturating_sub(1).min(3))
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn valid_checkpoint(checkpoint: &Checkpoint) -> bool {
    checkpoint.records.len() <= MAX_RECORDS
        && checkpoint.records.iter().all(|(id, record)| {
            id == &record.subscription_id
                && valid_id(id)
                && record.reason_code.as_ref().is_none_or(|code| {
                    code.len() <= 48 && code.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
                })
        })
}
fn record_unknown(checkpoint: &mut Checkpoint, id: &str) {
    if let Some(record) = checkpoint.records.get_mut(id) {
        record.status = AttemptStatus::ResultUnknown;
        record.reason_code = Some("check-original-result".into());
    }
}
fn reconcile(checkpoint: &mut Checkpoint, profiles: &[SubscriptionProfile]) {
    checkpoint.records.retain(|id, record| {
        profiles.iter().any(|profile| {
            profile.id == *id
                && profile.managed_source.is_none()
                && (matches!(
                    record.status,
                    AttemptStatus::InProgress | AttemptStatus::ResultUnknown
                ) || (profile.enabled && profile.update_interval_secs.is_some()))
                && match record.status {
                    AttemptStatus::Confirmed => {
                        profile.last_sync_at_unix_ms == record.confirmed_last_sync_at_unix_ms
                    }
                    _ => profile.last_sync_at_unix_ms == record.baseline_last_sync_at_unix_ms,
                }
        })
    });
    for record in checkpoint.records.values_mut() {
        if record.status == AttemptStatus::InProgress {
            record.status = AttemptStatus::ResultUnknown;
            record.reason_code = Some("check-original-result".into());
        }
    }
}
fn select_due(
    profiles: &[SubscriptionProfile],
    checkpoint: &Checkpoint,
    now: u64,
) -> Vec<SubscriptionProfile> {
    let mut due: Vec<_> = profiles
        .iter()
        .filter(|profile| {
            profile.enabled
                && profile.managed_source.is_none()
                && valid_id(&profile.id)
                && profile.update_interval_secs.is_some_and(|interval| {
                    interval >= 60
                        && now
                            >= profile
                                .last_sync_at_unix_ms
                                .unwrap_or(0)
                                .saturating_add(interval.saturating_mul(1000))
                })
                && checkpoint.records.get(&profile.id).is_none_or(|record| {
                    !matches!(
                        record.status,
                        AttemptStatus::ResultUnknown | AttemptStatus::InProgress
                    ) && now >= record.next_eligible_at_unix_ms
                })
        })
        .cloned()
        .collect();
    due.sort_by_key(|profile| {
        checkpoint
            .records
            .get(&profile.id)
            .map_or(0, |record| record.started_at_unix_ms)
    });
    due
}
fn prune(checkpoint: &mut Checkpoint, keep: &str) -> bool {
    if checkpoint.records.len() < MAX_RECORDS || checkpoint.records.contains_key(keep) {
        return true;
    }
    let oldest = checkpoint
        .records
        .iter()
        .filter(|(_, record)| {
            !matches!(
                record.status,
                AttemptStatus::InProgress | AttemptStatus::ResultUnknown
            )
        })
        .min_by_key(|(_, record)| record.started_at_unix_ms)
        .map(|(id, _)| id.clone());
    if let Some(id) = oldest {
        checkpoint.records.remove(&id);
        true
    } else {
        false
    }
}
#[cfg(test)]
#[path = "mobile_subscription_scheduler/tests.rs"]
mod tests;
