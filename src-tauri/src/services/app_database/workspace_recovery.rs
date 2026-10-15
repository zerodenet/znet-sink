//! Ordinary workspace material only. No network, runtime commands or raw AppConfig.
use super::*;
use crate::configuration::workspace::digest;
use crate::models::config_workspace::{
    ConfigApplyStrategy, ConfigRuntimeIdentityView, ConfigTransactionReceipt,
};
use serde::{Deserialize, Serialize};
const MAX_BYTES: usize = 64 * 1024 * 1024;
pub(super) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS relational_epoch(id INTEGER PRIMARY KEY CHECK(id=1), revision INTEGER NOT NULL CHECK(revision>=0)) STRICT;
INSERT OR IGNORE INTO relational_epoch VALUES(1,0);
CREATE TABLE IF NOT EXISTS workspace_recovery_clock(id INTEGER PRIMARY KEY CHECK(id=1), revision INTEGER NOT NULL CHECK(revision>=0)) STRICT;
INSERT OR IGNORE INTO workspace_recovery_clock VALUES(1,0);
CREATE TABLE IF NOT EXISTS workspace_recovery (
 operation_id TEXT PRIMARY KEY CHECK(length(operation_id)=32), profile_id TEXT NOT NULL,
 stage TEXT NOT NULL, version INTEGER NOT NULL CHECK(version>0),
 metadata_json TEXT NOT NULL CHECK(json_valid(metadata_json) AND length(CAST(metadata_json AS BLOB))<=65536),
 candidate_json TEXT CHECK(candidate_json IS NULL OR (json_valid(candidate_json) AND length(CAST(candidate_json AS BLOB))<=16777216))
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS workspace_one_pending ON workspace_recovery(profile_id) WHERE stage NOT IN ('complete','rejected');
"#;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Stage {
    Prepared,
    SubmittedUnknown,
    RuntimeConfirmedPendingLocal,
    LocalCommitted,
    AppConfirmed,
    Complete,
    Rejected,
}
impl Stage {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Prepared | Self::SubmittedUnknown => "submitted_unknown",
            Self::RuntimeConfirmedPendingLocal | Self::LocalCommitted | Self::AppConfirmed => {
                "runtime_confirmed_pending_local"
            }
            Self::Complete => "confirmed",
            Self::Rejected => "rejected",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::SubmittedUnknown => "submitted_unknown",
            Self::RuntimeConfirmedPendingLocal => "runtime_confirmed_pending_local",
            Self::LocalCommitted => "local_committed",
            Self::AppConfirmed => "app_confirmed",
            Self::Complete => "complete",
            Self::Rejected => "rejected",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Evidence {
    pub transaction_id: u64,
    pub core_instance_id: String,
    pub config_revision: u64,
    pub started_at_unix_ms: u64,
    pub completed_at_unix_ms: u64,
    pub clock_regressed: bool,
}
impl Evidence {
    pub fn view(&self, meta: &Metadata) -> ConfigTransactionReceipt {
        ConfigTransactionReceipt {
            transaction_id: self.transaction_id,
            profile_id: meta.profile_id.clone(),
            strategy: ConfigApplyStrategy::HotReload,
            state: if meta.stage == Stage::Complete {
                "confirmed"
            } else {
                "runtime_confirmed"
            },
            effective_digest: meta.effective_digest.clone(),
            started_at_unix_ms: self.started_at_unix_ms,
            completed_at_unix_ms: self.completed_at_unix_ms,
            runtime_identity: ConfigRuntimeIdentityView {
                core_instance_id: self.core_instance_id.clone(),
                config_revision: self.config_revision,
            },
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Metadata {
    pub schema: u32,
    pub operation_id: String,
    pub profile_id: String,
    pub session: String,
    pub version: i64,
    pub stage: Stage,
    pub epoch: i64,
    pub created_at_unix_ms: u64,
    pub before_graph_digest: String,
    pub after_graph_digest: String,
    pub source_digest: String,
    pub candidate_digest: String,
    pub app_digest: String,
    pub rules_digest: String,
    pub effective_digest: String,
    pub capture_digest: String,
    #[serde(default)]
    pub app_history: Option<crate::services::file_history::HistoryToken>,
    #[serde(default)]
    pub rules_history: Option<crate::services::file_history::HistoryToken>,
    pub original_core_instance_id: String,
    pub original_config_revision: u64,
    pub original_scope_digest: String,
    pub original_generation: u64,
    pub receipt: Option<Evidence>,
}
#[derive(Clone)]
pub(crate) struct Record {
    pub meta: Metadata,
    pub candidate: Option<ProxyConfigProfile>,
}
fn fail() -> AppError {
    AppError {
        code: "workspace_recovery_storage",
        message: "工作区恢复记录未能持久确认；保留原操作屏障，不会重放".into(),
        details: None,
    }
}
pub(crate) fn changed() -> AppError {
    AppError::conflict(
        "workspace_recovery",
        "",
        "原工作区来源、关系、设置、规则或持久阶段已变化；未发布旧候选",
    )
}
fn safe<T>(x: rusqlite::Result<T>) -> AppResult<T> {
    x.map_err(|_| fail())
}
pub(super) fn advance_epoch(tx: &Transaction<'_>) -> AppResult<()> {
    let changed=safe(tx.execute("UPDATE relational_epoch SET revision=revision+1 WHERE id=1 AND revision<9223372036854775807",[]))?;
    if changed != 1 {
        return Err(fail());
    }
    Ok(())
}
fn epoch(c: &Connection) -> AppResult<i64> {
    safe(c.query_row(
        "SELECT revision FROM relational_epoch WHERE id=1",
        [],
        |r| r.get(0),
    ))
}
pub(crate) fn current_epoch(dir: &Path) -> AppResult<i64> {
    epoch(&open(dir).map_err(|_| fail())?)
}
fn hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
}
fn bounded(value: &str, limit: usize) -> bool {
    !value.is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
}
pub(super) fn validate_meta(m: &Metadata) -> AppResult<()> {
    if !matches!(m.schema, 1 | 2)
        || !hex(&m.operation_id, 32)
        || !hex(&m.session, 32)
        || m.version <= 0
        || m.epoch < 0
        || !bounded(&m.profile_id, 512)
        || !bounded(&m.original_core_instance_id, 512)
        || m.original_generation == 0
        || m.original_generation > 9_007_199_254_740_991
    {
        return Err(fail());
    }
    for hash in [
        &m.before_graph_digest,
        &m.after_graph_digest,
        &m.source_digest,
        &m.candidate_digest,
        &m.app_digest,
        &m.rules_digest,
        &m.effective_digest,
        &m.capture_digest,
        &m.original_scope_digest,
    ] {
        if !hex(hash, 64) {
            return Err(fail());
        }
    }
    match (&m.app_history, &m.rules_history) {
        (Some(app), Some(rules)) => {
            crate::services::file_history::validate_token(app)?;
            crate::services::file_history::validate_token(rules)?;
        }
        (None, None) if m.schema == 1 => {}
        _ => return Err(fail()),
    }
    let confirmed = matches!(
        m.stage,
        Stage::RuntimeConfirmedPendingLocal
            | Stage::LocalCommitted
            | Stage::AppConfirmed
            | Stage::Complete
    );
    if m.receipt.is_some() != confirmed {
        return Err(fail());
    }
    if let Some(r) = &m.receipt {
        if r.transaction_id == 0
            || r.core_instance_id != m.original_core_instance_id
            || r.config_revision < m.original_config_revision
            || r.clock_regressed
                != (r.started_at_unix_ms < m.created_at_unix_ms
                    || r.completed_at_unix_ms < r.started_at_unix_ms)
        {
            return Err(fail());
        }
    }
    Ok(())
}
fn row_meta(row: &rusqlite::Row<'_>) -> rusqlite::Result<(String, String, String, i64, String)> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
    ))
}
pub(super) fn metadata(row: (String, String, String, i64, String)) -> AppResult<Metadata> {
    let (id, profile, stage, version, raw) = row;
    if raw.len() > 65536 {
        return Err(fail());
    }
    let m: Metadata = serde_json::from_str(&raw).map_err(|_| fail())?;
    validate_meta(&m)?;
    if id != m.operation_id
        || profile != m.profile_id
        || stage != m.stage.key()
        || version != m.version
    {
        return Err(fail());
    }
    Ok(m)
}
fn read(c: &Connection, id: &str, body: bool) -> AppResult<Option<Record>> {
    // Decode/validate metadata and redundant columns before touching large bodies.
    let row=safe(c.query_row("SELECT operation_id,profile_id,stage,version,metadata_json FROM workspace_recovery WHERE operation_id=?1",[id],row_meta).optional())?;
    let Some(meta) = row.map(metadata).transpose()? else {
        return Ok(None);
    };
    let candidate = if body {
        let raw: Option<String> = safe(c.query_row(
            "SELECT candidate_json FROM workspace_recovery WHERE operation_id=?1",
            [id],
            |r| r.get(0),
        ))?;
        raw.map(|b| {
            if b.len() > 16 * 1024 * 1024 {
                return Err(fail());
            }
            serde_json::from_str::<ProxyConfigProfile>(&b).map_err(|_| fail())
        })
        .transpose()?
    } else {
        None
    };
    if body && candidate.is_none() != matches!(meta.stage, Stage::Complete | Stage::Rejected) {
        return Err(fail());
    }
    if let Some(p) = &candidate {
        if p.id != meta.profile_id
            || p.managed_source.is_some()
            || p.content.is_none()
            || p.kernel != "zero"
            || digest(p)? != meta.candidate_digest
        {
            return Err(fail());
        }
    }
    Ok(Some(Record { meta, candidate }))
}
pub(crate) fn load(dir: &Path, id: &str) -> AppResult<Option<Record>> {
    read(&open(dir).map_err(|_| fail())?, id, true)
}
pub(crate) fn load_meta(dir: &Path, id: &str) -> AppResult<Option<Metadata>> {
    Ok(read(&open(dir).map_err(|_| fail())?, id, false)?.map(|r| r.meta))
}
pub(crate) fn pending(dir: &Path, profile: &str) -> AppResult<Option<Metadata>> {
    let c = open(dir).map_err(|_| fail())?;
    // A damaged redundant stage/profile column must not hide a pending barrier.
    let mut query = safe(c.prepare(
        "SELECT operation_id,profile_id,stage,version,metadata_json FROM workspace_recovery",
    ))?;
    let rows = safe(query.query_map([], row_meta))?;
    let mut pending = None;
    for row in rows {
        let m = metadata(safe(row)?)?;
        if m.profile_id == profile && !matches!(m.stage, Stage::Complete | Stage::Rejected) {
            if pending.is_some() {
                return Err(fail());
            }
            pending = Some(m);
        }
    }
    Ok(pending)
}
fn next_version(tx: &Transaction<'_>) -> AppResult<i64> {
    if safe(tx.execute("UPDATE workspace_recovery_clock SET revision=revision+1 WHERE id=1 AND revision<9223372036854775807",[]))?!=1{return Err(fail());}
    safe(tx.query_row(
        "SELECT revision FROM workspace_recovery_clock WHERE id=1",
        [],
        |r| r.get(0),
    ))
}
fn validate_all(c: &Connection) -> AppResult<()> {
    let mut q = safe(c.prepare(
        "SELECT operation_id,profile_id,stage,version,metadata_json FROM workspace_recovery",
    ))?;
    for row in safe(q.query_map([], row_meta))? {
        metadata(safe(row)?)?;
    }
    Ok(())
}
fn write(tx: &Transaction<'_>, record: &Record, insert: bool) -> AppResult<()> {
    validate_all(tx)?;
    validate_meta(&record.meta)?;
    let raw = serde_json::to_string(&record.meta).map_err(|_| fail())?;
    let body = record
        .candidate
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|_| fail())?;
    if raw.len() > 65536 || body.as_ref().is_some_and(|b| b.len() > 16 * 1024 * 1024) {
        return Err(fail());
    }
    safe(tx.execute(if insert {"INSERT INTO workspace_recovery(operation_id,profile_id,stage,version,metadata_json,candidate_json) VALUES(?1,?2,?3,?4,?5,?6)"}else{"UPDATE workspace_recovery SET profile_id=?2,stage=?3,version=?4,metadata_json=?5,candidate_json=?6 WHERE operation_id=?1"},params![record.meta.operation_id,record.meta.profile_id,record.meta.stage.key(),record.meta.version,raw,body]))?;
    if matches!(record.meta.stage, Stage::Complete | Stage::Rejected) {
        safe(tx.execute("DELETE FROM workspace_recovery WHERE stage IN ('complete','rejected') AND operation_id NOT IN (SELECT operation_id FROM workspace_recovery WHERE stage IN ('complete','rejected') ORDER BY version DESC LIMIT 32)",[]))?;
    }
    let bytes:i64=safe(tx.query_row("SELECT COALESCE(SUM(length(CAST(operation_id AS BLOB))+length(CAST(profile_id AS BLOB))+length(CAST(metadata_json AS BLOB))+COALESCE(length(CAST(candidate_json AS BLOB)),0)),0) FROM workspace_recovery",[],|r|r.get(0)))?;
    if bytes > MAX_BYTES as i64 {
        return Err(fail());
    }
    Ok(())
}
fn graph(c: &Connection) -> AppResult<String> {
    digest(&(
        load_proxy_configs(c).map_err(|_| fail())?,
        load_subscriptions(c).map_err(|_| fail())?,
    ))
}
pub(crate) fn prepare(dir: &Path, mut record: Record) -> AppResult<Record> {
    let mut c = open(dir).map_err(|_| fail())?;
    import_legacy_json_once(&mut c, dir).map_err(|_| fail())?;
    let tx = immediate_transaction(&mut c).map_err(|_| fail())?;
    if graph(&tx)? != record.meta.before_graph_digest {
        return Err(changed());
    }
    let target = load_proxy_configs(&tx)?
        .into_iter()
        .find(|p| p.id == record.meta.profile_id)
        .ok_or_else(changed)?;
    if target.managed_source.is_some()
        || target.content.is_none()
        || target.kernel != "zero"
        || digest(&target)? != record.meta.source_digest
    {
        return Err(changed());
    }
    if record.meta.stage != Stage::Prepared
        || record.meta.receipt.is_some()
        || record.candidate.is_none()
    {
        return Err(fail());
    }
    let count: i64 = safe(tx.query_row(
        "SELECT count(*) FROM workspace_recovery WHERE stage NOT IN ('complete','rejected')",
        [],
        |r| r.get(0),
    ))?;
    if count >= 32 {
        return Err(fail());
    }
    if record.meta.epoch != epoch(&tx)? {
        return Err(changed());
    }
    record.meta.version = next_version(&tx)?;
    write(&tx, &record, true)?;
    commit_and_restrict(tx, dir).map_err(|_| fail())?;
    Ok(record)
}
fn transition(
    dir: &Path,
    id: &str,
    version: i64,
    from: &[Stage],
    change: impl FnOnce(&Transaction<'_>, &mut Record) -> AppResult<()>,
) -> AppResult<Record> {
    let mut c = open(dir).map_err(|_| fail())?;
    let tx = immediate_transaction(&mut c).map_err(|_| fail())?;
    let mut r = read(&tx, id, true)?.ok_or_else(changed)?;
    if r.meta.version != version || !from.contains(&r.meta.stage) {
        return Err(changed());
    }
    if epoch(&tx)? != r.meta.epoch {
        return Err(changed());
    }
    change(&tx, &mut r)?;
    r.meta.version = next_version(&tx)?;
    write(&tx, &r, false)?;
    commit_and_restrict(tx, dir).map_err(|_| fail())?;
    Ok(r)
}
pub(crate) fn submitted(dir: &Path, id: &str, version: i64) -> AppResult<Record> {
    transition(dir, id, version, &[Stage::Prepared], |_, r| {
        r.meta.stage = Stage::SubmittedUnknown;
        Ok(())
    })
}
pub(crate) fn rejected(dir: &Path, id: &str, version: i64) -> AppResult<Record> {
    transition(
        dir,
        id,
        version,
        &[Stage::Prepared, Stage::SubmittedUnknown],
        |_, r| {
            if r.meta.receipt.is_some() {
                return Err(changed());
            }
            r.meta.stage = Stage::Rejected;
            r.candidate = None;
            Ok(())
        },
    )
}
/// Persist authoritative historical evidence even if unrelated local writers
/// advanced the domain epoch. Every local publication still rejects that drift.
pub(crate) fn confirm(dir: &Path, id: &str, version: i64, evidence: Evidence) -> AppResult<Record> {
    let mut c = open(dir).map_err(|_| fail())?;
    let tx = immediate_transaction(&mut c).map_err(|_| fail())?;
    let mut r = read(&tx, id, true)?.ok_or_else(changed)?;
    if r.meta.version != version
        || r.meta.stage != Stage::SubmittedUnknown
        || evidence.core_instance_id != r.meta.original_core_instance_id
        || evidence.config_revision < r.meta.original_config_revision
    {
        return Err(changed());
    }
    r.meta.receipt = Some(evidence);
    r.meta.stage = Stage::RuntimeConfirmedPendingLocal;
    r.meta.version = next_version(&tx)?;
    write(&tx, &r, false)?;
    commit_and_restrict(tx, dir).map_err(|_| fail())?;
    Ok(r)
}
pub(crate) fn publish(dir: &Path, id: &str, version: i64) -> AppResult<Record> {
    transition(
        dir,
        id,
        version,
        &[Stage::RuntimeConfirmedPendingLocal],
        |tx, r| {
            if graph(tx)? != r.meta.before_graph_digest
                || r.meta.receipt.is_none()
                || r.meta.app_history.is_none()
                || r.meta.rules_history.is_none()
            {
                return Err(changed());
            }
            let mut profiles = load_proxy_configs(tx)?;
            let target = profiles
                .iter_mut()
                .find(|p| p.id == r.meta.profile_id)
                .ok_or_else(changed)?;
            if digest(target)? != r.meta.source_digest || target.managed_source.is_some() {
                return Err(changed());
            }
            *target = r.candidate.clone().ok_or_else(fail)?;
            replace_proxy_configs(tx, &profiles)?;
            if graph(tx)? != r.meta.after_graph_digest {
                return Err(changed());
            }
            r.meta.epoch = epoch(tx)?;
            r.meta.stage = Stage::LocalCommitted;
            Ok(())
        },
    )
}
pub(crate) fn app_confirmed(dir: &Path, id: &str, version: i64) -> AppResult<Record> {
    transition(dir, id, version, &[Stage::LocalCommitted], |tx, r| {
        if graph(tx)? != r.meta.after_graph_digest {
            return Err(changed());
        }
        r.meta.stage = Stage::AppConfirmed;
        Ok(())
    })
}
pub(crate) fn complete(dir: &Path, id: &str, version: i64) -> AppResult<Record> {
    let r = transition(dir, id, version, &[Stage::AppConfirmed], |tx, r| {
        if graph(tx)? != r.meta.after_graph_digest {
            return Err(changed());
        }
        r.meta.stage = Stage::Complete;
        r.candidate = None;
        Ok(())
    })?;
    Ok(r)
}

/// Hold SQLite's writer reservation through a pure synchronous memory projection.
/// No domain graph write occurs and the persistent epoch remains unchanged.
pub(crate) fn reconcile_graph<T>(
    dir: &Path,
    meta: &Metadata,
    publish: impl FnOnce(RelationalDomainData) -> T,
) -> AppResult<T> {
    let mut c = open(dir).map_err(|_| fail())?;
    let tx = immediate_transaction(&mut c).map_err(|_| fail())?;
    let stored = read(&tx, &meta.operation_id, false)?.ok_or_else(changed)?;
    let expected = if matches!(
        meta.stage,
        Stage::LocalCommitted | Stage::AppConfirmed | Stage::Complete
    ) {
        &meta.after_graph_digest
    } else {
        &meta.before_graph_digest
    };
    if stored.meta.version != meta.version || epoch(&tx)? != meta.epoch || graph(&tx)? != *expected
    {
        return Err(changed());
    }
    let data = RelationalDomainData {
        proxy_configs: load_proxy_configs(&tx)?,
        subscriptions: load_subscriptions(&tx)?,
    };
    // Read-only reservation: all fallible work precedes the memory projection.
    // Drop rolls back an unchanged transaction and releases the writer slot;
    // no fallible commit/restrict is allowed after publishing memory.
    let result = publish(data);
    drop(tx);
    Ok(result)
}

/// Fixed domain publication for ordinary local drafts. The epoch catches ABA
/// even when an external writer restored byte-identical relationship material.
pub(crate) fn save_domain_if_current(
    dir: &Path,
    expected_epoch: i64,
    expected_graph: &str,
    profiles: &[ProxyConfigProfile],
    subscriptions: &[SubscriptionProfile],
) -> AppResult<i64> {
    let mut c = open(dir).map_err(|_| fail())?;
    import_legacy_json_once(&mut c, dir).map_err(|_| fail())?;
    let tx = immediate_transaction(&mut c).map_err(|_| fail())?;
    validate_all(&tx)?;
    if epoch(&tx)? != expected_epoch || graph(&tx)? != expected_graph {
        return Err(changed());
    }
    replace_proxy_configs(&tx, profiles)?;
    replace_subscriptions(&tx, subscriptions)?;
    let next = epoch(&tx)?;
    commit_and_restrict(tx, dir).map_err(|_| fail())?;
    Ok(next)
}

pub(crate) fn latest(dir: &Path, profile: &str) -> AppResult<Option<Metadata>> {
    let c = open(dir).map_err(|_| fail())?;
    validate_all(&c)?;
    let row=safe(c.query_row("SELECT operation_id,profile_id,stage,version,metadata_json FROM workspace_recovery WHERE profile_id=?1 ORDER BY version DESC LIMIT 1",[profile],row_meta).optional())?;
    row.map(metadata).transpose()
}
