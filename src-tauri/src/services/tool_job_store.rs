//! Durable metadata for client-owned diagnostic jobs.
//!
//! Running work is never replayed after a crash. The restored record is marked
//! failed by `ToolRuntime`, so users can see what was interrupted and retry it
//! explicitly. Results are intentionally omitted and inputs are redacted before
//! persistence to keep diagnostic history out of the secrets boundary.

use std::{
    collections::BTreeSet,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use znet_client_core::capability::{Budget, Lease, Manager, Permission};

use super::app_database::storage::{self, Change};
use crate::{
    errors::{AppError, AppResult},
    kernel::redaction,
    models::tool_job::ToolJobSnapshot,
};

const KEY: &str = "history";
const MAX_PERSISTED_JOBS: usize = 64;
const MAX_VALUE_BYTES: usize = 60 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredJobs {
    schema_version: u32,
    jobs: Vec<ToolJobSnapshot>,
}

fn failure() -> AppError {
    AppError::internal("诊断任务状态无法保存或恢复，请检查客户端本地存储")
}

fn lease(manager: &Manager) -> AppResult<Lease> {
    let grants = BTreeSet::from([
        Permission::new("storage.read", "self"),
        Permission::new("storage.write", "self"),
    ]);
    let policy = manager
        .admit(
            "builtin.tool-jobs".to_owned(),
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
                resource_bytes: 65_544,
                timeout: Duration::from_secs(10),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .map_err(|_| failure())
}

pub(crate) fn load(manager: &Manager) -> AppResult<(Vec<ToolJobSnapshot>, Option<i64>)> {
    let lease = lease(manager)?;
    let entry = storage::get(&super::data_dir()?, &lease, KEY)
        .and_then(|value| value.take(&lease))
        .map_err(|_| failure())?;
    let Some(entry) = entry else {
        return Ok((Vec::new(), None));
    };
    let stored: StoredJobs = serde_json::from_slice(&entry.value).map_err(|_| failure())?;
    if stored.schema_version != 1 || stored.jobs.len() > MAX_PERSISTED_JOBS {
        return Err(failure());
    }
    Ok((stored.jobs, Some(entry.revision)))
}

pub(crate) fn save(
    manager: &Manager,
    snapshots: &[ToolJobSnapshot],
    revision: &mut Option<i64>,
) -> AppResult<()> {
    let mut jobs = snapshots
        .iter()
        .rev()
        .take(MAX_PERSISTED_JOBS)
        .cloned()
        .map(sanitized)
        .collect::<Vec<_>>();
    jobs.reverse();
    let value = loop {
        let value = serde_json::to_vec(&StoredJobs {
            schema_version: 1,
            jobs: jobs.clone(),
        })
        .map_err(|_| failure())?;
        if value.len() <= MAX_VALUE_BYTES {
            break value;
        }
        if jobs.is_empty() {
            return Err(failure());
        }
        jobs.remove(0);
    };

    let lease = lease(manager)?;
    let revisions = storage::change(
        &super::data_dir()?,
        &lease,
        &[Change::Put {
            key: KEY.into(),
            expected: *revision,
            value,
        }],
    )
    .and_then(|value| value.take(&lease))
    .map_err(|_| failure())?;
    *revision = revisions.last().copied();
    Ok(())
}

fn sanitized(mut job: ToolJobSnapshot) -> ToolJobSnapshot {
    job.subject = redaction::text(&job.subject);
    job.params = redaction::sensitive(&job.params);
    job.result = None;
    if let Some(error) = &mut job.error {
        error.message = redaction::text(&error.message);
        error.details = error.details.as_ref().map(redaction::sensitive);
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        client_core::{ClientScope, ConfigRevision, CoreInstanceId, ProfileId},
        models::tool_job::{ToolJobId, ToolJobKind, ToolJobState},
    };

    #[test]
    fn persisted_job_metadata_redacts_inputs_and_omits_results() {
        let job = ToolJobSnapshot {
            id: ToolJobId(1),
            scope: ClientScope {
                profile_id: Some(ProfileId("profile".into())),
                config_revision: ConfigRevision(1),
                core_instance_id: CoreInstanceId(1),
            },
            kind: ToolJobKind::DnsLookup,
            state: ToolJobState::Completed,
            subject: "https://secret.example/path".into(),
            params: serde_json::json!({"hostname":"example.test", "token":"secret"}),
            result: Some(serde_json::json!({"answer":"192.0.2.1"})),
            error: None,
            created_at_unix_ms: 1,
            started_at_unix_ms: Some(2),
            updated_at_unix_ms: 3,
            deadline_at_unix_ms: 4,
        };
        let stored = sanitized(job);
        assert_eq!(stored.subject, "[redacted-url]");
        assert_eq!(stored.params["token"], "[redacted]");
        assert!(stored.result.is_none());
    }
}
