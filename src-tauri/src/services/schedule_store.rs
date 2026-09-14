//! Durable retry checkpoints for existing client schedulers, not a task replay queue.
use super::app_database::storage::{self, Change};
use crate::errors::{AppError, AppResult};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use znet_client_core::capability::{Budget, Lease, Manager, Permission};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint<T> {
    schema_version: u32,
    retries: T,
}
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> AppResult<T> {
    let value: Checkpoint<T> = serde_json::from_slice(bytes).map_err(|_| failure())?;
    if value.schema_version != 1 {
        return Err(failure());
    }
    Ok(value.retries)
}

fn lease(manager: &Manager, scheduler: &str) -> AppResult<Lease> {
    // Native-only allowlist; no caller-provided plugin namespace or SQL.
    if !matches!(scheduler, "subscriptions" | "rule-sets") {
        return Err(failure());
    }
    let grants = BTreeSet::from([
        Permission::new("storage.read", "self"),
        Permission::new("storage.write", "self"),
    ]);
    let policy = manager
        .admit(
            format!("builtin.scheduler.{scheduler}"),
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
fn failure() -> AppError {
    AppError::internal("定时刷新状态无法保存或恢复，自动刷新已暂停；请检查本地存储后重启客户端")
}

pub(crate) async fn load<T: DeserializeOwned + Default + Send + 'static>(
    manager: Manager,
    scheduler: &'static str,
) -> AppResult<(T, Option<i64>)> {
    tauri::async_runtime::spawn_blocking(move || {
        let lease = lease(&manager, scheduler)?;
        read_checkpoint(&super::data_dir()?, &lease)
    })
    .await
    .map_err(|_| failure())?
}
fn read_checkpoint<T: DeserializeOwned + Default>(
    dir: &std::path::Path,
    lease: &Lease,
) -> AppResult<(T, Option<i64>)> {
    let entry = storage::get(dir, lease, "retry-state")
        .and_then(|v| v.take(lease))
        .map_err(|_| failure())?;
    match entry {
        None => Ok((T::default(), None)),
        Some(entry) => Ok((decode(&entry.value)?, Some(entry.revision))),
    }
}
pub(crate) async fn save<T: Serialize>(
    manager: Manager,
    scheduler: &'static str,
    data: &T,
    revision: &mut Option<i64>,
) -> AppResult<()> {
    let value = serde_json::to_vec(&Checkpoint {
        schema_version: 1,
        retries: data,
    })
    .map_err(|_| failure())?;
    let expected = *revision;
    let revisions = tauri::async_runtime::spawn_blocking(move || {
        let lease = lease(&manager, scheduler)?;
        storage::change(
            &super::data_dir()?,
            &lease,
            &[Change::Put {
                key: "retry-state".into(),
                expected,
                value,
            }],
        )
        .and_then(|v| v.take(&lease))
        .map_err(|_| failure())
    })
    .await
    .map_err(|_| failure())??;
    *revision = revisions.last().copied();
    Ok(())
}

#[cfg(test)]
mod tests;
