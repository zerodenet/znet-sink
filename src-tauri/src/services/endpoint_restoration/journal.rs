use super::{Binding, Record};
use crate::errors::{AppError, AppResult};
use crate::services::{atomic_file, common::lock};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

const MAX_BINDINGS: usize = 128;
const MAX_RECORDS: usize = 32;
const MAX_BYTES: u64 = 1024 * 1024;
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Journal {
    pub version: u32,
    pub bindings: BTreeMap<String, Binding>,
    pub records: BTreeMap<String, Record>,
}
#[derive(Default)]
pub struct EndpointRestoration {
    inner: Mutex<Option<(PathBuf, Journal)>>,
}
impl EndpointRestoration {
    pub(super) fn read(&self) -> AppResult<Journal> {
        let path = super::journal_path()?;
        self.read_at(&path)
    }
    fn read_at(&self, path: &Path) -> AppResult<Journal> {
        let mut current = lock(&self.inner, "endpoint_restoration")?;
        if let Some((existing, journal)) = current.as_ref() {
            if existing == path {
                return Ok(journal.clone());
            }
        }
        let journal = if path.exists() {
            let size = std::fs::metadata(path).map_err(io)?.len();
            if size > MAX_BYTES {
                return Err(AppError::invalid_argument("端点恢复记录超过存储预算"));
            }
            let data = std::fs::read(path).map_err(io)?;
            let journal: Journal = serde_json::from_slice(&data)
                .map_err(|_| AppError::invalid_argument("端点恢复记录损坏，不能自动重放"))?;
            if journal.version != 1
                || journal.bindings.len() > MAX_BINDINGS
                || journal.records.len() > MAX_RECORDS
            {
                return Err(AppError::invalid_argument("端点恢复记录版本或容量不兼容"));
            }
            journal
        } else {
            Journal {
                version: 1,
                ..Default::default()
            }
        };
        *current = Some((path.to_owned(), journal.clone()));
        Ok(journal)
    }
    pub(super) fn replace(&self, journal: Journal) -> AppResult<()> {
        let path = super::journal_path()?;
        self.replace_at(&path, journal)
    }
    fn replace_at(&self, path: &Path, journal: Journal) -> AppResult<()> {
        if journal.bindings.len() > MAX_BINDINGS || journal.records.len() > MAX_RECORDS {
            return Err(AppError::conflict(
                "endpoint_restoration",
                "",
                "恢复记录容量已满，请先核对未完成事务",
            ));
        }
        let bytes = serde_json::to_vec(&journal).map_err(|e| AppError::internal(e.to_string()))?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(AppError::invalid_argument("端点恢复记录超过存储预算"));
        }
        let mut current = lock(&self.inner, "endpoint_restoration")?;
        atomic_file::write(path, &bytes).map_err(io)?;
        *current = Some((path.to_owned(), journal));
        Ok(())
    }
}
fn io(error: std::io::Error) -> AppError {
    AppError {
        code: "endpoint_restoration_storage",
        message: format!("端点恢复记录存储失败：{error}"),
        details: None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restored_journal_keeps_unknown_phases_without_any_replay() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.json");
        let owner = EndpointRestoration::default();
        let mut journal = owner.read_at(&path).unwrap();
        assert_eq!(journal.version, 1);
        let mut record = super::super::tests::record();
        record.receipt.phase = crate::models::endpoint_restoration::RestorePhase::ResultUnknown;
        record.clear_started = true;
        journal.records.insert(record.plan.plan_id.clone(), record);
        owner.replace_at(&path, journal.clone()).unwrap();
        let restored = EndpointRestoration::default().read_at(&path).unwrap();
        assert_eq!(restored.version, 1);
        assert_eq!(
            restored.records["operation"].receipt.phase,
            crate::models::endpoint_restoration::RestorePhase::ResultUnknown
        );
        assert!(restored.records["operation"].clear_started);
        std::fs::write(&path, b"{\"version\":2,\"bindings\":{},\"records\":{}}").unwrap();
        assert!(EndpointRestoration::default().read_at(&path).is_err());
        std::fs::write(&path, b"corrupt").unwrap();
        assert!(EndpointRestoration::default().read_at(&path).is_err());
    }
}
