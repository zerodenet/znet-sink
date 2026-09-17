//! Durable, redacted history for client-owned probe jobs.

use std::{
    collections::BTreeSet,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use znet_client_core::{
    capability::{Budget, Lease, Manager, Permission},
    ProbeJobSnapshot,
};

use super::app_database::storage::{self, Change};
use crate::{
    errors::{AppError, AppResult},
    kernel::redaction,
};

const KEY: &str = "history";
const MAX_PERSISTED_JOBS: usize = 64;
const MAX_VALUE_BYTES: usize = 60 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredJobs {
    schema_version: u32,
    jobs: Vec<ProbeJobSnapshot>,
}

fn failure() -> AppError {
    AppError::internal("节点测速任务状态无法保存或恢复，请检查客户端本地存储")
}

fn lease(manager: &Manager) -> AppResult<Lease> {
    let grants = BTreeSet::from([
        Permission::new("storage.read", "self"),
        Permission::new("storage.write", "self"),
    ]);
    let policy = manager
        .admit(
            "builtin.probe-jobs".to_owned(),
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

pub(crate) fn load(manager: &Manager) -> AppResult<(Vec<ProbeJobSnapshot>, Option<i64>)> {
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
    snapshots: &[ProbeJobSnapshot],
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

fn sanitized(mut job: ProbeJobSnapshot) -> ProbeJobSnapshot {
    for tag in &mut job.target_tags {
        *tag = redaction::text(tag);
    }
    for result in &mut job.results {
        result.target_tag = redaction::text(&result.target_tag);
        result.message = result.message.as_deref().map(redaction::text);
    }
    job
}
