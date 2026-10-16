//! Receipt-bound ordinary subscription publication. Recovery has no network,
//! runtime apply, capture transition, download, or system-proxy side effects.
use super::file_history::{self, Domain, Guard, HistoryToken, WriteReceipt};
use crate::{
    configuration::capture_apply::Receipt,
    errors::{AppError, AppResult},
    kernel::zero::queries::KernelRuntimeIdentity,
    models::{
        app_config::AppConfig, proxy_config::ProxyConfigProfile, rule_set::RuleSetProfile,
        subscription::SubscriptionProfile,
    },
    services::{app_database::ordinary_recovery as ledger, common::lock},
    state::app_state::AppState,
};
use ledger::{Record, Stage};
use std::{path::Path, sync::MutexGuard};

pub(crate) struct RecoveryOffer {
    pub recovery_id: String,
    pub expected_identity: Option<KernelRuntimeIdentity>,
}
fn changed() -> AppError {
    AppError::conflict(
        "subscription_recovery",
        "",
        "原订阅来源、配置、关系、设置或恢复记录已变化，未发布旧候选；请重新核对",
    )
}
fn pending() -> AppError {
    AppError {
        code: "subscription_local_recovery_pending",
        message: crate::services::subscription_reconciliation::UNKNOWN_APPLY_MESSAGE.into(),
        details: None,
    }
}
fn identity(record: &Record) -> Option<KernelRuntimeIdentity> {
    record
        .confirmed_identity
        .as_ref()
        .filter(|receipt| receipt.persistence == "runtime_only" && receipt.capture_validated)
        .map(|receipt| KernelRuntimeIdentity {
            core_instance_id: receipt.core_instance_id.clone(),
            config_revision: receipt.config_revision,
        })
}
fn projected_app(app: &AppConfig, profiles: &[ProxyConfigProfile]) -> AppConfig {
    let mut projected = app.clone();
    projected.local_proxy.source_proxy_config_id = None;
    let retained = profiles.iter().map(|profile| profile.id.as_str()).collect();
    crate::configuration::endpoints::prune(&mut projected, &retained);
    projected
}

/// Lock order is shared with workspace/endpoint recovery. File guards are never
/// held across await or followed by acquiring any AppState domain mutex.
struct Locked<'a> {
    profiles: MutexGuard<'a, Vec<ProxyConfigProfile>>,
    subscriptions: MutexGuard<'a, Vec<SubscriptionProfile>>,
    rules: MutexGuard<'a, Vec<RuleSetProfile>>,
    app: MutexGuard<'a, AppConfig>,
    app_history: Guard,
    rules_history: Guard,
    active_changed: bool,
}
fn with_locked<T>(
    state: &AppState,
    dir: &Path,
    app_path: &Path,
    action: impl FnOnce(&mut Locked<'_>) -> AppResult<T>,
) -> AppResult<T> {
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let subscriptions = lock(state.subscriptions(), "subscription")?;
    let rules = lock(state.rule_sets(), "rule_set")?;
    let app = lock(state.app_config(), "app_config")?;
    let app_history = file_history::acquire(app_path, Domain::AppConfig)?;
    let rules_history = file_history::acquire(&dir.join("rule-sets.json"), Domain::RuleSets)?;
    let mut locked = Locked {
        profiles,
        subscriptions,
        rules,
        app,
        app_history,
        rules_history,
        active_changed: false,
    };
    let result = action(&mut locked);
    let next = locked
        .active_changed
        .then(|| locked.profiles.iter().find(|p| p.active).cloned());
    drop(locked);
    if let Some(next) = next {
        state.client_core_configuration_committed(next.as_ref());
    }
    result
}
impl Locked<'_> {
    /// Pending owned writes are only probed here. No read/offer operation repairs
    /// history metadata or manufactures a completed-write checkpoint.
    fn check_files(&self, record: &Record) -> AppResult<Option<HistoryToken>> {
        let before = record.expected_app_history.as_ref().ok_or_else(changed)?;
        let rules = record
            .expected_rule_sets_history
            .as_ref()
            .ok_or_else(changed)?;
        self.rules_history.require(rules)?;
        if ledger::fingerprint(&super::domain_store::read_rule_sets_locked(
            &self.rules_history,
        )?)? != record.expected_rule_sets_fingerprint
            || ledger::fingerprint(&*self.rules)? != record.expected_rule_sets_fingerprint
        {
            return Err(changed());
        }
        let actual = super::app_config_store::read_current_locked(&self.app_history)?;
        let actual_hash = ledger::fingerprint(&actual)?;
        if let Some(receipt) = &record.app_projection_receipt {
            self.app_history.require(&receipt.after)?;
            if actual_hash != record.projected_app_fingerprint {
                return Err(changed());
            }
            // Also binds last-write ownership, not merely an equal token/value.
            if self.app_history.owned_token(&record.recovery_id, before)? != receipt.after {
                return Err(changed());
            }
            return Ok(Some(receipt.after.clone()));
        }
        if self.app_history.require(before).is_ok() {
            if actual_hash != record.expected_app_fingerprint {
                return Err(changed());
            }
            return Ok(None);
        }
        if record.stage != Stage::LocalCommitted
            || identity(record).is_none()
            || record.expected_app_fingerprint == record.projected_app_fingerprint
            || actual_hash != record.projected_app_fingerprint
        {
            return Err(changed());
        }
        self.app_history
            .owned_token(&record.recovery_id, before)
            .map(Some)
    }
    fn reconcile(&mut self, dir: &Path, record: &Record) -> AppResult<()> {
        self.check_files(record)?;
        let authoritative = match record.stage {
            Stage::Prepared | Stage::ApplyPending | Stage::RuntimeConfirmed => {
                Some(record.expected_graph_fingerprint.as_str())
            }
            Stage::LocalCommitted | Stage::Complete => {
                record.published_graph_fingerprint.as_deref()
            }
        }
        .ok_or_else(changed)?;
        let actual = super::app_database::load_domain_data(dir)?;
        if ledger::graph_fingerprint(&actual.proxy_configs, &actual.subscriptions)? != authoritative
        {
            return Err(changed());
        }
        let memory = ledger::graph_fingerprint(&self.profiles, &self.subscriptions)?;
        if memory == authoritative {
            return Ok(());
        }
        if memory != record.preparation_graph_fingerprint
            && memory != record.expected_graph_fingerprint
            && record.local_published_graph_fingerprint.as_deref() != Some(memory.as_str())
            && record.published_graph_fingerprint.as_deref() != Some(memory.as_str())
        {
            return Err(changed());
        }
        let previous_active = self
            .profiles
            .iter()
            .find(|p| p.active)
            .map(ledger::fingerprint)
            .transpose()?;
        let next_active = actual
            .proxy_configs
            .iter()
            .find(|p| p.active)
            .map(ledger::fingerprint)
            .transpose()?;
        self.active_changed |= previous_active != next_active;
        *self.profiles = actual.proxy_configs;
        *self.subscriptions = actual.subscriptions;
        Ok(())
    }
    fn evaluate(&self, record: &Record, expected: u64) -> AppResult<()> {
        self.check_files(record)?;
        let source = self
            .subscriptions
            .iter()
            .find(|s| s.id == record.subscription_id)
            .ok_or_else(changed)?;
        if source.managed_source.is_some()
            || (source.updated_at_unix_ms != expected
                && !(record.stage == Stage::Complete
                    && expected == record.barrier_source_updated_at_unix_ms))
        {
            return Err(changed());
        }
        let graph = ledger::graph_fingerprint(&self.profiles, &self.subscriptions)?;
        let source_hash = ledger::fingerprint(source)?;
        let matches = match record.stage {
            Stage::Prepared | Stage::ApplyPending | Stage::RuntimeConfirmed => {
                graph == record.expected_graph_fingerprint
                    && source_hash == record.expected_source_fingerprint
            }
            Stage::LocalCommitted => {
                Some(graph) == record.published_graph_fingerprint
                    && Some(source_hash) == record.published_source_fingerprint
            }
            Stage::Complete => {
                Some(graph) == record.published_graph_fingerprint
                    && source_hash == record.next_subscription_fingerprint
            }
        };
        let app_hash = ledger::fingerprint(&*self.app)?;
        let app_matches = match record.stage {
            Stage::Prepared | Stage::ApplyPending | Stage::RuntimeConfirmed => {
                app_hash == record.expected_app_fingerprint
            }
            Stage::LocalCommitted => {
                app_hash == record.expected_app_fingerprint
                    || app_hash == record.projected_app_fingerprint
            }
            Stage::Complete => app_hash == record.projected_app_fingerprint,
        };
        if !matches || !app_matches {
            return Err(changed());
        }
        Ok(())
    }
}

/// Independent of the legacy error marker: a crash or partial local phase must
/// not make an original dispatch eligible for another download/apply.
pub(crate) fn has_pending(state: &AppState, id: &str) -> AppResult<bool> {
    let _ = state;
    ledger::has_pending(&super::data_dir()?, id)
}
pub(crate) fn require_sync_allowed(state: &AppState, id: &str) -> AppResult<()> {
    if has_pending(state, id)? {
        return Err(pending());
    }
    Ok(())
}

/// Caller holds proxy_config_operation and the source's in-flight claim.
/// The DB atomically installs the exact UNKNOWN source and prepared candidate.
pub(crate) fn prepare_locked(
    state: &AppState,
    expected_source: &SubscriptionProfile,
    next_profile: ProxyConfigProfile,
    next_subscription: SubscriptionProfile,
    effective: &serde_json::Value,
    capture: &serde_json::Value,
    expected_app: &AppConfig,
    expected_rules: &str,
) -> AppResult<Record> {
    prepare_at(
        state,
        &super::data_dir()?,
        expected_source,
        next_profile,
        next_subscription,
        effective,
        capture,
        expected_app,
        expected_rules,
    )
}
fn prepare_at(
    state: &AppState,
    dir: &Path,
    expected_source: &SubscriptionProfile,
    next_profile: ProxyConfigProfile,
    next_subscription: SubscriptionProfile,
    effective: &serde_json::Value,
    capture: &serde_json::Value,
    expected_app: &AppConfig,
    expected_rules: &str,
) -> AppResult<Record> {
    with_locked(state, dir, &dir.join("app-config.json"), |locked| {
        let profiles = &*locked.profiles;
        let subscriptions = &mut *locked.subscriptions;
        let source = subscriptions
            .iter_mut()
            .find(|source| source.id == expected_source.id)
            .ok_or_else(changed)?;
        if ledger::fingerprint(source)? != ledger::fingerprint(expected_source)? {
            return Err(changed());
        }
        let app = locked.app.clone();
        if &app != expected_app || ledger::fingerprint(&*locked.rules)? != expected_rules {
            return Err(changed());
        }
        if super::app_config_store::read_current_locked(&locked.app_history)? != app
            || ledger::fingerprint(&super::domain_store::read_rule_sets_locked(
                &locked.rules_history,
            )?)? != expected_rules
        {
            return Err(changed());
        }
        let expected_app_history = locked.app_history.token()?;
        let expected_rule_sets_history = locked.rules_history.token()?;
        let mut projected_profiles = profiles.clone();
        if next_profile.active {
            for profile in &mut projected_profiles {
                profile.active = false;
            }
        }
        match projected_profiles
            .iter_mut()
            .find(|profile| profile.id == next_profile.id)
        {
            Some(profile) => *profile = next_profile.clone(),
            None => projected_profiles.push(next_profile.clone()),
        }
        let input = ledger::Prepare {
            recovery_id: super::common::generated_store_id("subscription-recovery"),
            expected_source: expected_source.clone(),
            expected_profile: profiles
                .iter()
                .find(|profile| profile.id == next_profile.id)
                .cloned(),
            expected_graph_fingerprint: ledger::graph_fingerprint(&profiles, &subscriptions)?,
            expected_app_fingerprint: ledger::fingerprint(&app)?,
            expected_app_history,
            expected_rule_sets_history,
            expected_rule_sets_fingerprint: expected_rules.into(),
            projected_app_fingerprint: ledger::fingerprint(&projected_app(
                &app,
                &projected_profiles,
            ))?,
            effective_config_fingerprint: ledger::fingerprint(effective)?,
            capture_plan_fingerprint: ledger::fingerprint(capture)?,
            next_profile,
            next_subscription,
            created_at_unix_ms: super::common::now_unix_ms(),
        };
        let prepared = ledger::prepare(dir, input)?;
        let source = subscriptions
            .iter_mut()
            .find(|source| source.id == expected_source.id)
            .ok_or_else(changed)?;
        *source = prepared.barrier_source;
        Ok(prepared.record)
    })
}

pub(crate) fn mark_apply_pending_locked(state: &AppState, record: Record) -> AppResult<Record> {
    let dir = super::data_dir()?;
    with_locked(
        state,
        &dir,
        &super::app_config_store::default_config_path()?,
        |locked| {
            locked.evaluate(&record, record.barrier_source_updated_at_unix_ms)?;
            ledger::mark_apply_pending(&dir, &record.recovery_id, record.version)
        },
    )
}
/// Runtime evidence comes only from the actual capture_apply transaction.
pub(crate) fn confirm_locked(
    state: &AppState,
    record: &Record,
    receipt: &Receipt,
) -> AppResult<Record> {
    let _ = state;
    let evidence = confirmation_evidence(record, receipt)?;
    let dir = super::data_dir()?;
    // Persist the original confirmed runtime fact even if local files changed
    // after dispatch. Such changes still prohibit every local publication.
    ledger::confirm(&dir, &record.recovery_id, record.version, evidence).map_err(|_| pending())
}
/// No persisted receipt can be manufactured from a latest-runtime query.
/// Both fingerprints must belong to the returned capture_apply transaction.
pub(crate) fn confirmation_evidence(
    record: &Record,
    receipt: &Receipt,
) -> AppResult<ledger::ConfirmedIdentity> {
    if ledger::fingerprint(receipt.applied_config())? != record.effective_config_fingerprint
        || ledger::fingerprint(receipt.applied_capture_parameters())?
            != record.capture_plan_fingerprint
    {
        return Err(pending());
    }
    Ok(ledger::ConfirmedIdentity {
        core_instance_id: receipt.identity.core_instance_id.clone(),
        config_revision: receipt.identity.config_revision,
        persistence: "runtime_only".into(),
        capture_validated: true,
    })
}

/// Inspector holds the per-source claim and configuration operation lock. A
/// completed record is retained for exact-ID retry, not as a fresh sync intent.
pub(crate) fn inspect_record(
    state: &AppState,
    id: &str,
    expected: u64,
) -> AppResult<Option<RecoveryOffer>> {
    inspect_at(state, &super::data_dir()?, id, expected)
}
fn inspect_at(
    state: &AppState,
    dir: &Path,
    id: &str,
    expected: u64,
) -> AppResult<Option<RecoveryOffer>> {
    let record = ledger::latest_for_subscription(dir, id)?;
    let Some(record) = record else {
        return Ok(None);
    };
    let checked = with_locked(state, dir, &dir.join("app-config.json"), |locked| {
        locked.reconcile(dir, &record)?;
        locked.evaluate(&record, expected)
    });
    if let Err(error) = checked {
        if record.stage == Stage::Complete {
            return Ok(None);
        }
        return Err(error);
    }

    Ok(Some(RecoveryOffer {
        recovery_id: record.recovery_id.clone(),
        expected_identity: identity(&record),
    }))
}

pub(crate) fn finish_confirmed_locked(
    state: &AppState,
    record: Record,
) -> AppResult<SubscriptionProfile> {
    finish_confirmed_at(
        state,
        &super::data_dir()?,
        &super::app_config_store::default_config_path()?,
        record,
        || Ok(()),
    )
}
fn finish_confirmed_at(
    state: &AppState,
    dir: &Path,
    app_path: &Path,
    record: Record,
    after_complete: impl FnOnce() -> AppResult<()>,
) -> AppResult<SubscriptionProfile> {
    let observed = identity(&record).ok_or_else(pending)?;
    publish_at_with_delivery_gate(state, dir, app_path, record, observed, None, after_complete)
        .map_err(|_| pending())
}

/// Only local publication. The caller holds both domain operation guards and
/// has queried the original bound runtime identity immediately before this call.
pub(crate) fn recover_locked(
    state: &AppState,
    id: &str,
    expected: u64,
    recovery_id: &str,
    observed: &KernelRuntimeIdentity,
) -> AppResult<SubscriptionProfile> {
    let dir = super::data_dir()?;
    let record = ledger::load(&dir, recovery_id)?
        .filter(|record| record.subscription_id == id)
        .ok_or_else(changed)?;
    publish_at(
        state,
        &dir,
        &super::app_config_store::default_config_path()?,
        record,
        observed.clone(),
        Some(expected),
    )
}

fn publish_at(
    state: &AppState,
    dir: &Path,
    app_path: &Path,
    record: Record,
    observed: KernelRuntimeIdentity,
    expected: Option<u64>,
) -> AppResult<SubscriptionProfile> {
    publish_at_with_delivery_gate(state, dir, app_path, record, observed, expected, || Ok(()))
}
fn publish_at_with_delivery_gate(
    state: &AppState,
    dir: &Path,
    app_path: &Path,
    mut record: Record,
    observed: KernelRuntimeIdentity,
    expected: Option<u64>,
    after_complete: impl FnOnce() -> AppResult<()>,
) -> AppResult<SubscriptionProfile> {
    if identity(&record).as_ref() != Some(&observed) {
        return Err(changed());
    }
    with_locked(state, dir, app_path, |locked| {
        locked.reconcile(dir, &record)?;
        let expected = expected.unwrap_or(if record.stage == Stage::Complete {
            record.published_source_updated_at_unix_ms
        } else {
            record.barrier_source_updated_at_unix_ms
        });
        locked.evaluate(&record, expected)?;
        if record.stage == Stage::Complete {
            return locked
                .subscriptions
                .iter()
                .find(|s| s.id == record.subscription_id)
                .cloned()
                .ok_or_else(changed);
        }
        if !matches!(
            record.stage,
            Stage::RuntimeConfirmed | Stage::LocalCommitted
        ) {
            return Err(pending());
        }
        if record.stage == Stage::RuntimeConfirmed {
            record = ledger::publish_local(dir, &record.recovery_id, record.version)
                .map_err(|_| pending())?;
            locked.reconcile(dir, &record)?;
        }
        locked.evaluate(&record, record.barrier_source_updated_at_unix_ms)?;
        let before = record.expected_app_history.clone().ok_or_else(changed)?;
        if record.expected_app_fingerprint != record.projected_app_fingerprint {
            if record.app_projection_receipt.is_none() {
                let receipt = if let Some(after) = locked.check_files(&record)? {
                    // Explicit recovery, original runtime identity was checked above.
                    // Only original owner metadata is reconciled; never rewrite bytes.
                    if locked
                        .app_history
                        .confirm_owned(&record.recovery_id, &before)?
                        != after
                    {
                        return Err(changed());
                    }
                    WriteReceipt {
                        before,
                        after,
                        owner_operation: Some(record.recovery_id.clone()),
                    }
                } else {
                    let candidate = projected_app(&locked.app, &locked.profiles);
                    if ledger::fingerprint(&candidate)? != record.projected_app_fingerprint {
                        return Err(changed());
                    }
                    locked.app_history.require(&before)?;
                    super::app_config_store::save_owned_locked(
                        &mut locked.app_history,
                        &candidate,
                        &record.recovery_id,
                    )
                    .map_err(|_| pending())?
                };
                record = ledger::record_app_projection(
                    dir,
                    &record.recovery_id,
                    record.version,
                    receipt,
                )
                .map_err(|_| pending())?;
            }
            let actual = super::app_config_store::read_current_locked(&locked.app_history)?;
            if ledger::fingerprint(&actual)? != record.projected_app_fingerprint {
                return Err(changed());
            }
            *locked.app = actual;
        }
        locked.evaluate(&record, record.barrier_source_updated_at_unix_ms)?;
        record =
            ledger::complete(dir, &record.recovery_id, record.version).map_err(|_| pending())?;
        after_complete()?;
        locked.reconcile(dir, &record)?;
        locked.evaluate(&record, record.barrier_source_updated_at_unix_ms)?;
        locked
            .subscriptions
            .iter()
            .find(|s| s.id == record.subscription_id)
            .cloned()
            .ok_or_else(changed)
    })
}

#[cfg(test)]
#[path = "ordinary_subscription_recovery_tests.rs"]
pub(crate) mod tests;
