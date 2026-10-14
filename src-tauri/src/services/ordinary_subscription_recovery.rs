//! Receipt-bound ordinary subscription publication. Recovery has no network,
//! runtime apply, capture transition, download, or system-proxy side effects.
use crate::{
    configuration::capture_apply::Receipt,
    errors::{AppError, AppResult},
    kernel::zero::queries::KernelRuntimeIdentity,
    models::{
        app_config::AppConfig, proxy_config::ProxyConfigProfile, subscription::SubscriptionProfile,
    },
    services::{app_database::ordinary_recovery as ledger, common::lock},
    state::app_state::AppState,
};
use ledger::{Record, Stage};
use std::path::Path;

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
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let mut subscriptions = lock(state.subscriptions(), "subscription")?;
    let source = subscriptions
        .iter_mut()
        .find(|source| source.id == expected_source.id)
        .ok_or_else(changed)?;
    if ledger::fingerprint(source)? != ledger::fingerprint(expected_source)? {
        return Err(changed());
    }
    let app = lock(state.app_config(), "app_config")?.clone();
    if &app != expected_app
        || ledger::fingerprint(&*lock(state.rule_sets(), "rule_set")?)? != expected_rules
    {
        return Err(changed());
    }
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
        expected_rule_sets_fingerprint: expected_rules.into(),
        projected_app_fingerprint: ledger::fingerprint(&projected_app(&app, &projected_profiles))?,
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
}

pub(crate) fn mark_apply_pending_locked(state: &AppState, record: Record) -> AppResult<Record> {
    evaluate(state, &record, record.barrier_source_updated_at_unix_ms)?;
    ledger::mark_apply_pending(&super::data_dir()?, &record.recovery_id, record.version)
}
/// The only production confirmation writer consumes evidence from the actual
/// capture_apply return. Readable/latest runtime identity alone is insufficient.
pub(crate) fn confirm_locked(
    state: &AppState,
    record: &Record,
    receipt: &Receipt,
) -> AppResult<Record> {
    evaluate(state, record, record.barrier_source_updated_at_unix_ms)?;
    let evidence = confirmation_evidence(record, receipt)?;
    ledger::confirm(
        &super::data_dir()?,
        &record.recovery_id,
        record.version,
        evidence,
    )
    .map_err(|_| pending())
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

/// Promote only DB states that this exact journal transaction already owns.
/// It never refreshes an arbitrary new graph over concurrent user edits.
fn reconcile_owned_graph_at(state: &AppState, dir: &Path, record: &Record) -> AppResult<()> {
    let mut profiles = lock(state.proxy_configs(), "proxy_config")?;
    let mut subscriptions = lock(state.subscriptions(), "subscription")?;
    let authoritative = match record.stage {
        Stage::Prepared | Stage::ApplyPending | Stage::RuntimeConfirmed => {
            Some(record.expected_graph_fingerprint.as_str())
        }
        Stage::LocalCommitted | Stage::Complete => record.published_graph_fingerprint.as_deref(),
    }
    .ok_or_else(changed)?;
    let actual = super::app_database::load_domain_data(dir)?;
    if ledger::graph_fingerprint(&actual.proxy_configs, &actual.subscriptions)? != authoritative {
        return Err(changed());
    }
    let memory = ledger::graph_fingerprint(&profiles, &subscriptions)?;
    if memory == authoritative {
        return Ok(());
    }
    let known = memory == record.preparation_graph_fingerprint
        || memory == record.expected_graph_fingerprint
        || record.local_published_graph_fingerprint.as_deref() == Some(memory.as_str())
        || record.published_graph_fingerprint.as_deref() == Some(memory.as_str());
    if !known {
        return Err(changed());
    }
    let previous_active = profiles
        .iter()
        .find(|profile| profile.active)
        .map(ledger::fingerprint)
        .transpose()?;
    let next_active = actual
        .proxy_configs
        .iter()
        .find(|profile| profile.active)
        .cloned();
    let active_changed =
        previous_active != next_active.as_ref().map(ledger::fingerprint).transpose()?;
    *profiles = actual.proxy_configs;
    *subscriptions = actual.subscriptions;
    drop(profiles);
    drop(subscriptions);
    if active_changed {
        state.client_core_configuration_committed(next_active.as_ref());
    }
    Ok(())
}

fn evaluate(state: &AppState, record: &Record, expected: u64) -> AppResult<()> {
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let subscriptions = lock(state.subscriptions(), "subscription")?;
    let source = subscriptions
        .iter()
        .find(|source| source.id == record.subscription_id)
        .ok_or_else(changed)?;
    if source.managed_source.is_some()
        || source.updated_at_unix_ms != expected
            && !(record.stage == Stage::Complete
                && expected == record.barrier_source_updated_at_unix_ms)
    {
        return Err(changed());
    }
    let graph = ledger::graph_fingerprint(&profiles, &subscriptions)?;
    let source_hash = ledger::fingerprint(source)?;
    match record.stage {
        Stage::Prepared | Stage::ApplyPending | Stage::RuntimeConfirmed => {
            if graph != record.expected_graph_fingerprint
                || source_hash != record.expected_source_fingerprint
            {
                return Err(changed());
            }
        }
        Stage::LocalCommitted => {
            if Some(graph) != record.published_graph_fingerprint
                || Some(source_hash) != record.published_source_fingerprint
            {
                return Err(changed());
            }
        }
        Stage::Complete => {
            if Some(graph) != record.published_graph_fingerprint
                || source_hash != record.next_subscription_fingerprint
            {
                return Err(changed());
            }
        }
    }
    if ledger::fingerprint(&*lock(state.rule_sets(), "rule_set")?)?
        != record.expected_rule_sets_fingerprint
    {
        return Err(changed());
    }
    let app_hash = ledger::fingerprint(&*lock(state.app_config(), "app_config")?)?;
    let allowed = match record.stage {
        Stage::Prepared | Stage::ApplyPending | Stage::RuntimeConfirmed => {
            app_hash == record.expected_app_fingerprint
        }
        Stage::LocalCommitted => {
            app_hash == record.expected_app_fingerprint
                || app_hash == record.projected_app_fingerprint
        }
        Stage::Complete => app_hash == record.projected_app_fingerprint,
    };
    if !allowed {
        return Err(changed());
    }
    Ok(())
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
    if let Err(error) = reconcile_owned_graph_at(state, dir, &record) {
        if record.stage == Stage::Complete {
            return Ok(None);
        }
        return Err(error);
    }
    if let Err(error) = evaluate(state, &record, expected) {
        // An unrelated later legacy result cannot borrow an old receipt, and
        // that old tombstone must not suppress its read-only unknown facts.
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
    reconcile_owned_graph_at(state, &dir, &record)?;
    evaluate(state, &record, expected)?;
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
    reconcile_owned_graph_at(state, dir, &record)?;
    let expected = expected.unwrap_or_else(|| {
        if record.stage == Stage::Complete {
            record.published_source_updated_at_unix_ms
        } else {
            record.barrier_source_updated_at_unix_ms
        }
    });
    evaluate(state, &record, expected)?;
    if record.stage == Stage::Complete {
        return lock(state.subscriptions(), "subscription")?
            .iter()
            .find(|source| source.id == record.subscription_id)
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
        let mut profiles = lock(state.proxy_configs(), "proxy_config")?;
        let mut subscriptions = lock(state.subscriptions(), "subscription")?;
        if ledger::graph_fingerprint(&profiles, &subscriptions)?
            != record.expected_graph_fingerprint
        {
            return Err(changed());
        }
        record = ledger::publish_local(dir, &record.recovery_id, record.version)
            .map_err(|_| pending())?;
        let actual = super::app_database::load_domain_data(dir)?;
        if Some(ledger::graph_fingerprint(
            &actual.proxy_configs,
            &actual.subscriptions,
        )?) != record.published_graph_fingerprint
        {
            return Err(changed());
        }
        let previous_active = profiles
            .iter()
            .find(|profile| profile.active)
            .map(ledger::fingerprint)
            .transpose()?;
        let next_active = actual
            .proxy_configs
            .iter()
            .find(|profile| profile.active)
            .cloned();
        let active_changed =
            previous_active != next_active.as_ref().map(ledger::fingerprint).transpose()?;
        *profiles = actual.proxy_configs;
        *subscriptions = actual.subscriptions;
        drop(profiles);
        drop(subscriptions);
        if active_changed {
            state.client_core_configuration_committed(next_active.as_ref());
        }
    }
    evaluate(state, &record, record.barrier_source_updated_at_unix_ms)?;
    let app = lock(state.app_config(), "app_config")?.clone();
    let profiles = lock(state.proxy_configs(), "proxy_config")?.clone();
    if ledger::fingerprint(&app)? != record.projected_app_fingerprint {
        let candidate = projected_app(&app, &profiles);
        if ledger::fingerprint(&candidate)? != record.projected_app_fingerprint {
            return Err(changed());
        }
        super::app_config::compare_and_replace_local_at(state, &app, candidate, app_path)
            .map_err(|_| pending())?;
    }
    // No system proxy retarget, HTTP, runtime apply, stop/start or compensation.
    let mut profiles = lock(state.proxy_configs(), "proxy_config")?;
    let mut subscriptions = lock(state.subscriptions(), "subscription")?;
    if Some(ledger::graph_fingerprint(&profiles, &subscriptions)?)
        != record.published_graph_fingerprint
    {
        return Err(changed());
    }
    let app_guard = lock(state.app_config(), "app_config")?;
    if ledger::fingerprint(&*app_guard)? != record.projected_app_fingerprint {
        return Err(changed());
    }
    let rules_guard = lock(state.rule_sets(), "rule_set")?;
    if ledger::fingerprint(&*rules_guard)? != record.expected_rule_sets_fingerprint {
        return Err(changed());
    }
    record = ledger::complete(dir, &record.recovery_id, record.version).map_err(|_| pending())?;
    after_complete()?;
    let actual = super::app_database::load_domain_data(dir)?;
    if Some(ledger::graph_fingerprint(
        &actual.proxy_configs,
        &actual.subscriptions,
    )?) != record.published_graph_fingerprint
    {
        return Err(changed());
    }
    let source = actual
        .subscriptions
        .iter()
        .find(|source| source.id == record.subscription_id)
        .cloned()
        .ok_or_else(changed)?;
    if ledger::fingerprint(&source)? != record.next_subscription_fingerprint {
        return Err(changed());
    }
    *profiles = actual.proxy_configs;
    *subscriptions = actual.subscriptions;
    Ok(source)
}

#[cfg(test)]
#[path = "ordinary_subscription_recovery_tests.rs"]
pub(crate) mod tests;
