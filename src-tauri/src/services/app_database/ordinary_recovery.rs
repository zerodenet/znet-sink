//! Trusted ordinary-material recovery ledger. Never used for managed/protected
//! bodies or exported as a frontend DTO. Database phases cannot issue network IO.
use super::*;
use crate::services::file_history::{HistoryToken, WriteReceipt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) const MAX_CANDIDATE_BYTES: usize = 16 * 1024 * 1024;
const MAX_LEDGER_BYTES: i64 = 64 * 1024 * 1024;
const MAX_PENDING: i64 = 32;
const MAX_COMPLETED: i64 = 32;
const MAX_METADATA_BYTES: usize = 64 * 1024;
pub(super) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS ordinary_recovery_clock (
    id INTEGER PRIMARY KEY CHECK(id=1), revision INTEGER NOT NULL CHECK(revision>=0)
) STRICT;
INSERT OR IGNORE INTO ordinary_recovery_clock VALUES(1,0);
CREATE TABLE IF NOT EXISTS ordinary_subscription_recovery (
    recovery_id TEXT PRIMARY KEY NOT NULL CHECK(length(recovery_id) BETWEEN 1 AND 128),
    subscription_id TEXT NOT NULL CHECK(length(subscription_id) BETWEEN 1 AND 512),
    stage TEXT NOT NULL CHECK(stage IN ('prepared','apply-pending','runtime-confirmed','local-committed','complete')),
    revision INTEGER NOT NULL CHECK(revision>0),
    metadata_json TEXT NOT NULL CHECK(json_valid(metadata_json) AND length(CAST(metadata_json AS BLOB))<=65536),
    candidate_json TEXT CHECK(candidate_json IS NULL OR (json_valid(candidate_json) AND length(CAST(candidate_json AS BLOB))<=16777216))
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS ordinary_recovery_one_pending_source
    ON ordinary_subscription_recovery(subscription_id) WHERE stage!='complete';
"#;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Stage {
    Prepared,
    ApplyPending,
    RuntimeConfirmed,
    LocalCommitted,
    Complete,
}
impl Stage {
    fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::ApplyPending => "apply-pending",
            Self::RuntimeConfirmed => "runtime-confirmed",
            Self::LocalCommitted => "local-committed",
            Self::Complete => "complete",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfirmedIdentity {
    pub core_instance_id: String,
    pub config_revision: u64,
    pub persistence: String,
    pub capture_validated: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub schema: u32,
    pub recovery_id: String,
    pub subscription_id: String,
    pub target_profile_id: String,
    pub version: i64,
    pub stage: Stage,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
    pub barrier_source_updated_at_unix_ms: u64,
    pub published_source_updated_at_unix_ms: u64,
    pub published_source_fingerprint: Option<String>,
    pub expected_source_fingerprint: String,
    pub expected_profile_fingerprint: Option<String>,
    pub preparation_graph_fingerprint: String,
    pub expected_graph_fingerprint: String,
    pub local_published_graph_fingerprint: Option<String>,
    pub published_graph_fingerprint: Option<String>,
    pub expected_rule_sets_fingerprint: String,
    pub expected_app_fingerprint: String,
    #[serde(default)]
    pub expected_app_history: Option<HistoryToken>,
    #[serde(default)]
    pub expected_rule_sets_history: Option<HistoryToken>,
    #[serde(default)]
    pub app_projection_receipt: Option<WriteReceipt>,
    pub projected_app_fingerprint: String,
    pub effective_config_fingerprint: String,
    pub capture_plan_fingerprint: String,
    pub next_profile_fingerprint: String,
    pub next_subscription_fingerprint: String,
    pub confirmed_identity: Option<ConfirmedIdentity>,
    // Complete rows keep only a bounded metadata tombstone, not large bodies.
    pub next_profile: Option<ProxyConfigProfile>,
    pub next_subscription: Option<SubscriptionProfile>,
}
impl std::fmt::Debug for Record {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrdinaryRecoveryRecord")
            .field("stage", &self.stage)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}
pub(crate) struct Prepare {
    pub recovery_id: String,
    pub expected_source: SubscriptionProfile,
    pub expected_profile: Option<ProxyConfigProfile>,
    pub expected_graph_fingerprint: String,
    pub expected_rule_sets_fingerprint: String,
    pub expected_app_fingerprint: String,
    pub expected_app_history: HistoryToken,
    pub expected_rule_sets_history: HistoryToken,
    pub projected_app_fingerprint: String,
    pub effective_config_fingerprint: String,
    pub capture_plan_fingerprint: String,
    pub next_profile: ProxyConfigProfile,
    pub next_subscription: SubscriptionProfile,
    pub created_at_unix_ms: u64,
}
pub(crate) struct Prepared {
    pub record: Record,
    pub barrier_source: SubscriptionProfile,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Candidate {
    profile: ProxyConfigProfile,
    subscription: SubscriptionProfile,
}
fn failure() -> AppError {
    AppError {
        code: "subscription_recovery_storage",
        message: "普通订阅恢复记录无法读取、校验或保存；未自动重放，请检查本地存储".into(),
        details: None,
    }
}
fn conflict() -> AppError {
    AppError {
        code: "conflict",
        message: "普通订阅来源、材料、关系或恢复记录已变化，请重新核对".into(),
        details: None,
    }
}
fn safe<T>(value: rusqlite::Result<T>) -> AppResult<T> {
    value.map_err(|_| failure())
}
fn db_open(dir: &Path) -> AppResult<Connection> {
    open(dir).map_err(|_| failure())
}
fn finish_tx(tx: Transaction<'_>, dir: &Path) -> AppResult<()> {
    commit_and_restrict(tx, dir).map_err(|_| failure())
}
pub(crate) fn fingerprint(value: &impl Serialize) -> AppResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|_| failure())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
pub(crate) fn graph_fingerprint(
    profiles: &[ProxyConfigProfile],
    subscriptions: &[SubscriptionProfile],
) -> AppResult<String> {
    fingerprint(&(profiles, subscriptions))
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
fn valid_recovery_id(value: &str) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}
fn valid_id(value: &str, limit: usize) -> bool {
    !value.is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
}
fn identity_valid(value: &ConfirmedIdentity) -> bool {
    valid_id(&value.core_instance_id, 512)
        && value.persistence == "runtime_only"
        && value.capture_validated
}
fn ordinary(profile: &ProxyConfigProfile, source: &SubscriptionProfile) -> bool {
    profile.managed_source.is_none()
        && source.managed_source.is_none()
        && profile.kernel == "zero"
        && source.kernel == "zero"
        && source.target_proxy_config_id.as_deref() == Some(&profile.id)
        && profile
            .content
            .as_ref()
            .is_some_and(|content| zero_config::RuntimeConfig::parse(&content.to_string()).is_ok())
}
fn valid_history(token: &HistoryToken) -> bool {
    token.epoch > 0
        && token.store_id.len() == 32
        && token
            .store_id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        && valid_hash(&token.file_id)
}
fn history_bound(record: &Record) -> bool {
    record
        .expected_app_history
        .as_ref()
        .is_some_and(valid_history)
        && record
            .expected_rule_sets_history
            .as_ref()
            .is_some_and(valid_history)
}
fn valid_projection(record: &Record, receipt: &WriteReceipt) -> bool {
    record.expected_app_history.as_ref() == Some(&receipt.before)
        && valid_history(&receipt.after)
        && receipt.before.store_id == receipt.after.store_id
        && receipt.before.file_id == receipt.after.file_id
        && receipt.before.epoch.checked_add(1) == Some(receipt.after.epoch)
        && receipt.owner_operation.as_deref() == Some(record.recovery_id.as_str())
        && matches!(record.stage, Stage::LocalCommitted | Stage::Complete)
        && record.confirmed_identity.is_some()
        && record.expected_app_fingerprint != record.projected_app_fingerprint
}
fn validate_metadata(record: &Record) -> AppResult<()> {
    match (
        &record.expected_app_history,
        &record.expected_rule_sets_history,
    ) {
        (None, None) if record.app_projection_receipt.is_none() => {} // historical only
        (Some(app), Some(rules)) if valid_history(app) && valid_history(rules) => {}
        _ => return Err(failure()),
    }
    if record
        .app_projection_receipt
        .as_ref()
        .is_some_and(|r| !valid_projection(record, r))
    {
        return Err(failure());
    }
    if record.schema != 1
        || record.version <= 0
        || !valid_recovery_id(&record.recovery_id)
        || !valid_id(&record.subscription_id, 512)
        || !valid_id(&record.target_profile_id, 512)
        || ![
            &record.expected_source_fingerprint,
            &record.preparation_graph_fingerprint,
            &record.expected_graph_fingerprint,
            &record.expected_rule_sets_fingerprint,
            &record.expected_app_fingerprint,
            &record.projected_app_fingerprint,
            &record.effective_config_fingerprint,
            &record.capture_plan_fingerprint,
            &record.next_profile_fingerprint,
            &record.next_subscription_fingerprint,
        ]
        .iter()
        .all(|hash| valid_hash(hash))
        || record
            .expected_profile_fingerprint
            .as_deref()
            .is_some_and(|hash| !valid_hash(hash))
        || record
            .published_source_fingerprint
            .as_deref()
            .is_some_and(|hash| !valid_hash(hash))
        || record
            .local_published_graph_fingerprint
            .as_deref()
            .is_some_and(|hash| !valid_hash(hash))
        || record
            .published_graph_fingerprint
            .as_deref()
            .is_some_and(|hash| !valid_hash(hash))
        || record
            .confirmed_identity
            .as_ref()
            .is_some_and(|identity| !identity_valid(identity))
    {
        return Err(failure());
    }
    if record.published_source_updated_at_unix_ms <= record.barrier_source_updated_at_unix_ms {
        return Err(failure());
    }
    if matches!(
        record.stage,
        Stage::RuntimeConfirmed | Stage::LocalCommitted | Stage::Complete
    ) && record.confirmed_identity.is_none()
    {
        return Err(failure());
    }
    if matches!(record.stage, Stage::LocalCommitted | Stage::Complete)
        && (record.local_published_graph_fingerprint.is_none()
            || record.published_graph_fingerprint.is_none()
            || record.published_source_fingerprint.is_none())
    {
        return Err(failure());
    }
    Ok(())
}
fn validate(record: &Record) -> AppResult<()> {
    validate_metadata(record)?;
    match (
        record.stage,
        record.next_profile.as_ref(),
        record.next_subscription.as_ref(),
    ) {
        (Stage::Complete, None, None) => {}
        (Stage::Complete, _, _) => return Err(failure()),
        (_, Some(profile), Some(source))
            if ordinary(profile, source)
                && profile.id == record.target_profile_id
                && source.id == record.subscription_id
                && fingerprint(profile)? == record.next_profile_fingerprint
                && fingerprint(source)? == record.next_subscription_fingerprint => {}
        _ => return Err(failure()),
    }
    if matches!(
        record.stage,
        Stage::RuntimeConfirmed | Stage::LocalCommitted | Stage::Complete
    ) && record.confirmed_identity.is_none()
    {
        return Err(failure());
    }
    if matches!(record.stage, Stage::LocalCommitted | Stage::Complete)
        && (record.local_published_graph_fingerprint.is_none()
            || record.published_graph_fingerprint.is_none()
            || record.published_source_fingerprint.is_none())
    {
        return Err(failure());
    }
    Ok(())
}
fn encode(record: &Record) -> AppResult<(String, Option<String>)> {
    validate(record)?;
    let mut metadata = record.clone();
    let candidate = match (
        metadata.next_profile.take(),
        metadata.next_subscription.take(),
    ) {
        (Some(profile), Some(subscription)) => Some(
            serde_json::to_string(&Candidate {
                profile,
                subscription,
            })
            .map_err(|_| failure())?,
        ),
        (None, None) => None,
        _ => return Err(failure()),
    };
    let metadata = serde_json::to_string(&metadata).map_err(|_| failure())?;
    if metadata.len() > MAX_METADATA_BYTES
        || candidate
            .as_ref()
            .is_some_and(|value| value.len() > MAX_CANDIDATE_BYTES)
    {
        return Err(failure());
    }
    Ok((metadata, candidate))
}
fn decode(
    metadata: &str,
    candidate: Option<&str>,
    stage: &str,
    revision: i64,
    source_id: &str,
    id: &str,
) -> AppResult<Record> {
    if metadata.len() > MAX_METADATA_BYTES
        || candidate.is_some_and(|value| value.len() > MAX_CANDIDATE_BYTES)
    {
        return Err(failure());
    }
    let mut record: Record = serde_json::from_str(metadata).map_err(|_| failure())?;
    if record.next_profile.is_some() || record.next_subscription.is_some() {
        return Err(failure());
    }
    if let Some(candidate) = candidate {
        let candidate: Candidate = serde_json::from_str(candidate).map_err(|_| failure())?;
        record.next_profile = Some(candidate.profile);
        record.next_subscription = Some(candidate.subscription);
    }
    if record.stage.as_str() != stage
        || record.version != revision
        || record.subscription_id != source_id
        || record.recovery_id != id
    {
        return Err(failure());
    }
    validate(&record)?;
    Ok(record)
}
fn read(connection: &Connection, id: &str) -> AppResult<Option<Record>> {
    let row=safe(connection.query_row("SELECT metadata_json,candidate_json,stage,revision,subscription_id FROM ordinary_subscription_recovery WHERE recovery_id=?1",[id],|row|Ok((row.get::<_,String>(0)?,row.get::<_,Option<String>>(1)?,row.get::<_,String>(2)?,row.get::<_,i64>(3)?,row.get::<_,String>(4)?))).optional())?;
    row.map(|(metadata, candidate, stage, revision, source)| {
        decode(
            &metadata,
            candidate.as_deref(),
            &stage,
            revision,
            &source,
            id,
        )
    })
    .transpose()
}
fn next_version(tx: &Transaction<'_>) -> AppResult<i64> {
    let current: i64 = safe(tx.query_row(
        "SELECT revision FROM ordinary_recovery_clock WHERE id=1",
        [],
        |row| row.get(0),
    ))?;
    let next = current.checked_add(1).ok_or_else(failure)?;
    safe(tx.execute(
        "UPDATE ordinary_recovery_clock SET revision=?1 WHERE id=1",
        [next],
    ))?;
    Ok(next)
}
fn update(tx: &Transaction<'_>, record: &Record, expected: i64) -> AppResult<()> {
    let (metadata, candidate) = encode(record)?;
    if safe(tx.execute("UPDATE ordinary_subscription_recovery SET stage=?1,revision=?2,metadata_json=?3,candidate_json=?4 WHERE recovery_id=?5 AND revision=?6",params![record.stage.as_str(),record.version,metadata,candidate,record.recovery_id,expected]))?!=1 {return Err(conflict());}
    budget(tx)
}
#[derive(Clone, Copy)]
struct BudgetLimits {
    pending: i64,
    bytes: i64,
}
const PRODUCTION_BUDGET: BudgetLimits = BudgetLimits {
    pending: MAX_PENDING,
    bytes: MAX_LEDGER_BYTES,
};
fn budget(tx: &Transaction<'_>) -> AppResult<()> {
    budget_with_limits(tx, PRODUCTION_BUDGET)
}
fn budget_with_limits(tx: &Transaction<'_>, limits: BudgetLimits) -> AppResult<()> {
    let(pending,bytes):(i64,i64)=safe(tx.query_row("SELECT coalesce(sum(stage!='complete'),0),coalesce(sum(length(CAST(recovery_id AS BLOB))+length(CAST(subscription_id AS BLOB))+length(CAST(metadata_json AS BLOB))+coalesce(length(CAST(candidate_json AS BLOB)),0)),0) FROM ordinary_subscription_recovery",[],|row|Ok((row.get(0)?,row.get(1)?))))?;
    if pending > limits.pending || bytes > limits.bytes {
        return Err(failure());
    }
    Ok(())
}
fn load_graph(connection: &Connection) -> AppResult<RelationalDomainData> {
    Ok(RelationalDomainData {
        proxy_configs: load_proxy_configs(connection).map_err(|_| failure())?,
        subscriptions: load_subscriptions(connection).map_err(|_| failure())?,
    })
}
fn require_version(connection: &Connection, id: &str, expected: i64) -> AppResult<Record> {
    let record = read(connection, id)?.ok_or_else(conflict)?;
    if record.version != expected {
        return Err(conflict());
    }
    Ok(record)
}
pub(crate) fn load(dir: &Path, id: &str) -> AppResult<Option<Record>> {
    read(&db_open(dir)?, id)
}
pub(crate) fn latest_for_subscription(dir: &Path, id: &str) -> AppResult<Option<Record>> {
    let connection = db_open(dir)?;
    let id=safe(connection.query_row("SELECT recovery_id FROM ordinary_subscription_recovery WHERE subscription_id=?1 ORDER BY (stage!='complete') DESC,revision DESC LIMIT 1",[id],|row|row.get::<_,String>(0)).optional())?;
    id.map(|id| read(&connection, &id))
        .transpose()
        .map(Option::flatten)
}
/// Admission barrier: select only EXISTS, never deserialize a material body.
pub(crate) fn has_pending(dir: &Path, id: &str) -> AppResult<bool> {
    let connection = db_open(dir)?;
    safe(connection.query_row("SELECT EXISTS(SELECT 1 FROM ordinary_subscription_recovery WHERE subscription_id=?1 AND stage!='complete')",[id],|row|row.get(0)))
}
pub(crate) fn pending_for_subscription(dir: &Path, id: &str) -> AppResult<Option<Record>> {
    let connection = db_open(dir)?;
    let id=safe(connection.query_row("SELECT recovery_id FROM ordinary_subscription_recovery WHERE subscription_id=?1 AND stage!='complete'",[id],|row|row.get::<_,String>(0)).optional())?;
    id.map(|id| read(&connection, &id))
        .transpose()
        .map(Option::flatten)
}
/// Lightweight startup/inspection facts. No candidate_json is selected or cloned.
#[derive(Clone, Debug)]
pub(crate) struct Metadata {
    pub recovery_id: String,
    pub subscription_id: String,
    pub target_profile_id: String,
    pub version: i64,
    pub stage: Stage,
    pub created_at_unix_ms: u64,
    pub barrier_source_updated_at_unix_ms: u64,
    pub published_source_updated_at_unix_ms: u64,
    pub expected_source_fingerprint: String,
    pub expected_profile_fingerprint: Option<String>,
    pub preparation_graph_fingerprint: String,
    pub expected_graph_fingerprint: String,
    pub local_published_graph_fingerprint: Option<String>,
    pub published_graph_fingerprint: Option<String>,
    pub published_source_fingerprint: Option<String>,
    pub expected_rule_sets_fingerprint: String,
    pub expected_app_fingerprint: String,
    pub expected_app_history: Option<HistoryToken>,
    pub expected_rule_sets_history: Option<HistoryToken>,
    pub app_projection_receipt: Option<WriteReceipt>,
    pub projected_app_fingerprint: String,
    pub effective_config_fingerprint: String,
    pub capture_plan_fingerprint: String,
    pub next_profile_fingerprint: String,
    pub next_subscription_fingerprint: String,
    pub confirmed_identity: Option<ConfirmedIdentity>,
    pub candidate_present: bool,
}
fn metadata(connection: &Connection, id: &str) -> AppResult<Option<Metadata>> {
    let row=safe(connection.query_row("SELECT metadata_json,stage,revision,subscription_id,candidate_json IS NOT NULL FROM ordinary_subscription_recovery WHERE recovery_id=?1",[id],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?,row.get::<_,String>(3)?,row.get::<_,bool>(4)?))).optional())?;
    row.map(|(json, stage, version, source, candidate_present)| {
        if json.len() > MAX_METADATA_BYTES {
            return Err(failure());
        }
        let record: Record = serde_json::from_str(&json).map_err(|_| failure())?;
        if record.next_profile.is_some()
            || record.next_subscription.is_some()
            || record.recovery_id != id
            || record.subscription_id != source
            || record.version != version
            || record.stage.as_str() != stage
            || candidate_present != (record.stage != Stage::Complete)
        {
            return Err(failure());
        }
        validate_metadata(&record)?;
        Ok(Metadata {
            recovery_id: record.recovery_id,
            subscription_id: record.subscription_id,
            target_profile_id: record.target_profile_id,
            version,
            stage: record.stage,
            created_at_unix_ms: record.created_at_unix_ms,
            barrier_source_updated_at_unix_ms: record.barrier_source_updated_at_unix_ms,
            published_source_updated_at_unix_ms: record.published_source_updated_at_unix_ms,
            expected_source_fingerprint: record.expected_source_fingerprint,
            expected_profile_fingerprint: record.expected_profile_fingerprint,
            preparation_graph_fingerprint: record.preparation_graph_fingerprint,
            expected_graph_fingerprint: record.expected_graph_fingerprint,
            local_published_graph_fingerprint: record.local_published_graph_fingerprint,
            published_graph_fingerprint: record.published_graph_fingerprint,
            published_source_fingerprint: record.published_source_fingerprint,
            expected_rule_sets_fingerprint: record.expected_rule_sets_fingerprint,
            expected_app_fingerprint: record.expected_app_fingerprint,
            expected_app_history: record.expected_app_history,
            expected_rule_sets_history: record.expected_rule_sets_history,
            app_projection_receipt: record.app_projection_receipt,
            projected_app_fingerprint: record.projected_app_fingerprint,
            effective_config_fingerprint: record.effective_config_fingerprint,
            capture_plan_fingerprint: record.capture_plan_fingerprint,
            next_profile_fingerprint: record.next_profile_fingerprint,
            next_subscription_fingerprint: record.next_subscription_fingerprint,
            confirmed_identity: record.confirmed_identity,
            candidate_present,
        })
    })
    .transpose()
}
pub(crate) fn load_metadata(dir: &Path, id: &str) -> AppResult<Option<Metadata>> {
    metadata(&db_open(dir)?, id)
}
pub(crate) fn metadata_for_subscription(dir: &Path, id: &str) -> AppResult<Option<Metadata>> {
    let connection = db_open(dir)?;
    let id=safe(connection.query_row("SELECT recovery_id FROM ordinary_subscription_recovery WHERE subscription_id=?1 AND stage!='complete'",[id],|row|row.get::<_,String>(0)).optional())?;
    id.map(|id| metadata(&connection, &id))
        .transpose()
        .map(Option::flatten)
}
pub(crate) fn list_pending_metadata(dir: &Path) -> AppResult<Vec<Metadata>> {
    let connection = db_open(dir)?;
    let mut ids=safe(connection.prepare("SELECT recovery_id FROM ordinary_subscription_recovery WHERE stage!='complete' ORDER BY revision LIMIT 33"))?;
    let mut result = Vec::new();
    for id in safe(ids.query_map([], |row| row.get::<_, String>(0)))? {
        result.push(metadata(&connection, &safe(id)?)?.ok_or_else(failure)?);
        if result.len() > MAX_PENDING as usize {
            return Err(failure());
        }
    }
    Ok(result)
}
pub(crate) fn prepare(dir: &Path, input: Prepare) -> AppResult<Prepared> {
    prepare_with_limits(dir, input, PRODUCTION_BUDGET)
}
fn prepare_with_limits(
    dir: &Path,
    mut input: Prepare,
    limits: BudgetLimits,
) -> AppResult<Prepared> {
    if !valid_history(&input.expected_app_history)
        || !valid_history(&input.expected_rule_sets_history)
        || (input.expected_app_fingerprint != input.projected_app_fingerprint
            && input.expected_app_history.epoch.checked_add(1).is_none())
        || !valid_recovery_id(&input.recovery_id)
        || input
            .expected_source
            .target_proxy_config_id
            .as_ref()
            .is_some_and(|id| id != &input.next_profile.id)
        || input.expected_source.url != input.next_subscription.url
        || input.expected_source.user_agent != input.next_subscription.user_agent
        || input.expected_source.format != input.next_subscription.format
        || input.expected_source.kernel != input.next_subscription.kernel
        || input.expected_source.enabled != input.next_subscription.enabled
        || input.expected_source.name != input.next_subscription.name
        || input.expected_source.update_interval_secs
            != input.next_subscription.update_interval_secs
        || input.expected_source.managed_source.is_some()
        || input
            .expected_profile
            .as_ref()
            .is_some_and(|profile| profile.managed_source.is_some())
        || input.expected_source.id != input.next_subscription.id
        || !ordinary(&input.next_profile, &input.next_subscription)
        || input
            .expected_profile
            .as_ref()
            .is_some_and(|profile| profile.id != input.next_profile.id)
        || input.expected_source.last_error.as_deref()
            == Some(crate::services::subscription_reconciliation::UNKNOWN_APPLY_MESSAGE)
    {
        return Err(conflict());
    }
    let mut connection = db_open(dir)?;
    import_legacy_json_once(&mut connection, dir).map_err(|_| failure())?;
    let tx = immediate_transaction(&mut connection).map_err(|_| failure())?;
    let mut graph = load_graph(&tx)?;
    if graph_fingerprint(&graph.proxy_configs, &graph.subscriptions)?
        != input.expected_graph_fingerprint
    {
        return Err(conflict());
    }
    let source = graph
        .subscriptions
        .iter_mut()
        .find(|source| source.id == input.expected_source.id)
        .ok_or_else(conflict)?;
    if fingerprint(source)? != fingerprint(&input.expected_source)? {
        return Err(conflict());
    }
    let profile = graph
        .proxy_configs
        .iter()
        .find(|profile| profile.id == input.next_profile.id);
    let expected_profile_fingerprint = input
        .expected_profile
        .as_ref()
        .map(fingerprint)
        .transpose()?;
    if profile.map(fingerprint).transpose()? != expected_profile_fingerprint {
        return Err(conflict());
    }
    source.last_error =
        Some(crate::services::subscription_reconciliation::UNKNOWN_APPLY_MESSAGE.into());
    source.updated_at_unix_ms = input.created_at_unix_ms.max(
        source
            .updated_at_unix_ms
            .checked_add(1)
            .ok_or_else(failure)?,
    );
    let barrier_source = source.clone();
    input.next_subscription.updated_at_unix_ms = input.next_subscription.updated_at_unix_ms.max(
        barrier_source
            .updated_at_unix_ms
            .checked_add(1)
            .ok_or_else(failure)?,
    );
    input.next_subscription.last_error = None;
    let version = next_version(&tx)?;
    let record = Record {
        schema: 1,
        recovery_id: input.recovery_id,
        subscription_id: barrier_source.id.clone(),
        target_profile_id: input.next_profile.id.clone(),
        version,
        stage: Stage::Prepared,
        created_at_unix_ms: input.created_at_unix_ms,
        updated_at_unix_ms: input.created_at_unix_ms,
        barrier_source_updated_at_unix_ms: barrier_source.updated_at_unix_ms,
        published_source_updated_at_unix_ms: input.next_subscription.updated_at_unix_ms,
        published_source_fingerprint: None,
        expected_source_fingerprint: fingerprint(&barrier_source)?,
        expected_profile_fingerprint,
        preparation_graph_fingerprint: input.expected_graph_fingerprint,
        expected_graph_fingerprint: graph_fingerprint(&graph.proxy_configs, &graph.subscriptions)?,
        local_published_graph_fingerprint: None,
        published_graph_fingerprint: None,
        expected_rule_sets_fingerprint: input.expected_rule_sets_fingerprint,
        expected_app_fingerprint: input.expected_app_fingerprint,
        expected_app_history: Some(input.expected_app_history),
        expected_rule_sets_history: Some(input.expected_rule_sets_history),
        app_projection_receipt: None,
        projected_app_fingerprint: input.projected_app_fingerprint,
        effective_config_fingerprint: input.effective_config_fingerprint,
        capture_plan_fingerprint: input.capture_plan_fingerprint,
        next_profile_fingerprint: fingerprint(&input.next_profile)?,
        next_subscription_fingerprint: fingerprint(&input.next_subscription)?,
        confirmed_identity: None,
        next_profile: Some(input.next_profile),
        next_subscription: Some(input.next_subscription),
    };
    let (metadata, candidate) = encode(&record)?;
    safe(tx.execute("INSERT INTO ordinary_subscription_recovery(recovery_id,subscription_id,stage,revision,metadata_json,candidate_json) VALUES(?1,?2,?3,?4,?5,?6)",params![record.recovery_id,record.subscription_id,record.stage.as_str(),version,metadata,candidate]))?;
    budget_with_limits(&tx, limits)?;
    replace_subscriptions(&tx, &graph.subscriptions).map_err(|_| failure())?;
    finish_tx(tx, dir)?;
    Ok(Prepared {
        record,
        barrier_source,
    })
}
pub(crate) fn mark_apply_pending(dir: &Path, id: &str, expected: i64) -> AppResult<Record> {
    transition(
        dir,
        id,
        expected,
        Stage::Prepared,
        Stage::ApplyPending,
        None,
    )
}
pub(crate) fn confirm(
    dir: &Path,
    id: &str,
    expected: i64,
    identity: ConfirmedIdentity,
) -> AppResult<Record> {
    if !identity_valid(&identity) {
        return Err(failure());
    }
    transition(
        dir,
        id,
        expected,
        Stage::ApplyPending,
        Stage::RuntimeConfirmed,
        Some(identity),
    )
}
fn transition(
    dir: &Path,
    id: &str,
    expected: i64,
    from: Stage,
    to: Stage,
    identity: Option<ConfirmedIdentity>,
) -> AppResult<Record> {
    let mut connection = db_open(dir)?;
    let tx = immediate_transaction(&mut connection).map_err(|_| failure())?;
    let mut record = require_version(&tx, id, expected)?;
    if !history_bound(&record) || record.stage != from {
        return Err(conflict());
    }
    record.updated_at_unix_ms = record
        .updated_at_unix_ms
        .max(crate::services::common::now_unix_ms());
    record.stage = to;
    record.confirmed_identity = identity.or(record.confirmed_identity);
    record.version = next_version(&tx)?;
    update(&tx, &record, expected)?;
    finish_tx(tx, dir)?;
    Ok(record)
}
pub(crate) fn publish_local(dir: &Path, id: &str, expected: i64) -> AppResult<Record> {
    let mut connection = db_open(dir)?;
    let tx = immediate_transaction(&mut connection).map_err(|_| failure())?;
    let mut record = require_version(&tx, id, expected)?;
    if !history_bound(&record) || record.stage != Stage::RuntimeConfirmed {
        return Err(conflict());
    }
    let mut graph = load_graph(&tx)?;
    if graph_fingerprint(&graph.proxy_configs, &graph.subscriptions)?
        != record.expected_graph_fingerprint
    {
        return Err(conflict());
    }
    let source = graph
        .subscriptions
        .iter_mut()
        .find(|source| source.id == record.subscription_id)
        .ok_or_else(conflict)?;
    if source.managed_source.is_some() || fingerprint(source)? != record.expected_source_fingerprint
    {
        return Err(conflict());
    }
    let profile = graph
        .proxy_configs
        .iter()
        .find(|profile| profile.id == record.target_profile_id);
    if profile.map(fingerprint).transpose()? != record.expected_profile_fingerprint
        || profile.is_some_and(|profile| profile.managed_source.is_some())
    {
        return Err(conflict());
    }
    let next_profile = record.next_profile.clone().ok_or_else(failure)?;
    // Local publication is not final completion: keep the durable barrier and
    // original successful-check facts until the separate app projection confirms.
    source.target_proxy_config_id = Some(record.target_profile_id.clone());
    record.published_source_fingerprint = Some(fingerprint(source)?);
    if next_profile.active {
        for profile in &mut graph.proxy_configs {
            profile.active = false;
        }
    }
    match graph
        .proxy_configs
        .iter_mut()
        .find(|profile| profile.id == record.target_profile_id)
    {
        Some(profile) => *profile = next_profile,
        None => graph.proxy_configs.push(next_profile),
    }
    replace_proxy_configs(&tx, &graph.proxy_configs).map_err(|_| failure())?;
    replace_subscriptions(&tx, &graph.subscriptions).map_err(|_| failure())?;
    record.updated_at_unix_ms = record
        .updated_at_unix_ms
        .max(crate::services::common::now_unix_ms());
    record.stage = Stage::LocalCommitted;
    record.version = next_version(&tx)?;
    record.published_graph_fingerprint = Some(graph_fingerprint(
        &graph.proxy_configs,
        &graph.subscriptions,
    )?);
    record.local_published_graph_fingerprint = record.published_graph_fingerprint.clone();
    update(&tx, &record, expected)?;
    finish_tx(tx, dir)?;
    Ok(record)
}
/// Checkpoint only an actual owned file write/metadata confirmation. This does
/// not issue file IO, runtime commands or infer a receipt from a current value.
pub(crate) fn record_app_projection(
    dir: &Path,
    id: &str,
    expected: i64,
    receipt: WriteReceipt,
) -> AppResult<Record> {
    let mut connection = db_open(dir)?;
    let tx = immediate_transaction(&mut connection).map_err(|_| failure())?;
    let mut record = require_version(&tx, id, expected)?;
    if !history_bound(&record) || !valid_projection(&record, &receipt) {
        return Err(conflict());
    }
    if let Some(existing) = &record.app_projection_receipt {
        if existing == &receipt {
            return Ok(record);
        }
        return Err(conflict());
    }
    record.app_projection_receipt = Some(receipt);
    record.updated_at_unix_ms = record
        .updated_at_unix_ms
        .max(crate::services::common::now_unix_ms());
    record.version = next_version(&tx)?;
    update(&tx, &record, expected)?;
    finish_tx(tx, dir)?;
    Ok(record)
}
pub(crate) fn complete(dir: &Path, id: &str, expected: i64) -> AppResult<Record> {
    let mut connection = db_open(dir)?;
    let tx = immediate_transaction(&mut connection).map_err(|_| failure())?;
    let mut record = require_version(&tx, id, expected)?;
    if !history_bound(&record) {
        return Err(conflict());
    }
    if record.stage == Stage::Complete {
        return Ok(record);
    }
    if record.stage != Stage::LocalCommitted
        || (record.expected_app_fingerprint != record.projected_app_fingerprint
            && record.app_projection_receipt.is_none())
    {
        return Err(conflict());
    }
    let mut graph = load_graph(&tx)?;
    if Some(graph_fingerprint(
        &graph.proxy_configs,
        &graph.subscriptions,
    )?) != record.published_graph_fingerprint
    {
        return Err(conflict());
    }
    let source = graph
        .subscriptions
        .iter_mut()
        .find(|source| source.id == record.subscription_id)
        .ok_or_else(conflict)?;
    if Some(fingerprint(source)?) != record.published_source_fingerprint
        || source.managed_source.is_some()
    {
        return Err(conflict());
    }
    *source = record.next_subscription.clone().ok_or_else(failure)?;
    record.published_source_fingerprint = Some(fingerprint(source)?);
    replace_subscriptions(&tx, &graph.subscriptions).map_err(|_| failure())?;
    record.published_graph_fingerprint = Some(graph_fingerprint(
        &graph.proxy_configs,
        &graph.subscriptions,
    )?);
    record.updated_at_unix_ms = record
        .updated_at_unix_ms
        .max(crate::services::common::now_unix_ms());
    record.stage = Stage::Complete;
    record.version = next_version(&tx)?;
    record.next_profile = None;
    record.next_subscription = None;
    update(&tx, &record, expected)?;
    safe(tx.execute("DELETE FROM ordinary_subscription_recovery WHERE stage='complete' AND recovery_id NOT IN (SELECT recovery_id FROM ordinary_subscription_recovery WHERE stage='complete' ORDER BY revision DESC LIMIT ?1)",[MAX_COMPLETED]))?;
    finish_tx(tx, dir)?;
    Ok(record)
}

#[cfg(test)]
#[path = "ordinary_recovery/tests.rs"]
mod tests;
