//! Capture-owned metadata journal. No IPC body, error text or material is stored.
use crate::{
    client_core::ClientScope,
    errors::{AppError, AppResult},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub(crate) const RECORD_LIMIT: usize = 32;
pub(crate) const BYTE_LIMIT: usize = 64 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapturePhase {
    Capturing,
    Stopped,
    Interrupted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureConfirmation {
    Confirmed,
    ResultUnknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureEndReason {
    Duration,
    Capacity,
    User,
    Cleanup,
    StorageFailure,
    ScopeChanged,
    ProcessRestart,
    JournalFailure,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureRecord {
    pub session_id: String,
    pub scope: Option<ClientScope>,
    pub phase: CapturePhase,
    pub confirmation: CaptureConfirmation,
    pub started_at_ms: u64,
    pub expires_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub duration_seconds: u32,
    pub maximum_bytes: u64,
    pub stored_bytes: u64,
    pub stored_frames: u64,
    pub queue_drops_observed: u64,
    pub end_reason: Option<CaptureEndReason>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Ledger {
    schema_version: u32,
    records: Vec<CaptureRecord>,
}

#[derive(Clone)]
pub(crate) struct CaptureRecoveryStore {
    path: PathBuf,
    publication: Arc<Mutex<u64>>,
}
fn unavailable() -> AppError {
    AppError {
        code: "capture_recovery_unavailable",
        message: "捕获恢复账本不可读、格式不支持或存盘未确认；请核对原记录或显式清理捕获恢复记录"
            .into(),
        details: None,
    }
}
fn validate(records: &[CaptureRecord]) -> AppResult<()> {
    if records.len() > RECORD_LIMIT {
        return Err(unavailable());
    }
    let mut identities = std::collections::BTreeSet::new();
    for record in records {
        let valid_id = record
            .session_id
            .strip_prefix("capture-")
            .is_some_and(|id| {
                id.len() == 32
                    && id
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            });
        let valid_scope = record
            .scope
            .as_ref()
            .and_then(|scope| scope.profile_id.as_ref())
            .is_none_or(|profile| !profile.0.is_empty() && profile.0.len() <= 256);
        let terminal_matches = match record.phase {
            CapturePhase::Capturing => {
                record.confirmation == CaptureConfirmation::Confirmed
                    && record.end_reason.is_none()
                    && record.ended_at_ms.is_none()
            }
            CapturePhase::Stopped => {
                record.confirmation == CaptureConfirmation::Confirmed
                    && record.ended_at_ms.is_some()
                    && matches!(
                        record.end_reason,
                        Some(
                            CaptureEndReason::Duration
                                | CaptureEndReason::Capacity
                                | CaptureEndReason::User
                                | CaptureEndReason::Cleanup
                                | CaptureEndReason::StorageFailure
                                | CaptureEndReason::ScopeChanged
                        )
                    )
            }
            CapturePhase::Interrupted => {
                record.confirmation == CaptureConfirmation::ResultUnknown
                    && record.ended_at_ms.is_none()
                    && matches!(
                        record.end_reason,
                        Some(CaptureEndReason::ProcessRestart | CaptureEndReason::JournalFailure)
                    )
            }
        };
        if !valid_id
            || !valid_scope
            || !terminal_matches
            || !identities.insert(&record.session_id)
            || !(1..=300).contains(&record.duration_seconds)
            || !(1024 * 1024..=10 * 1024 * 1024).contains(&record.maximum_bytes)
            || record.maximum_bytes % (1024 * 1024) != 0
            || record.stored_bytes > record.maximum_bytes
            || record.stored_frames > record.stored_bytes
            || record.expires_at_ms
                != record
                    .started_at_ms
                    .saturating_add(u64::from(record.duration_seconds) * 1000)
        {
            return Err(unavailable());
        }
    }
    Ok(())
}
impl CaptureRecoveryStore {
    #[cfg(test)]
    pub(crate) fn hold_publication_for_test(&self) -> std::sync::MutexGuard<'_, u64> {
        self.publication.lock().unwrap()
    }
    pub(crate) fn new(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
            publication: Arc::new(Mutex::new(0)),
        }
    }
    pub(crate) fn load(&self) -> AppResult<Vec<CaptureRecord>> {
        let metadata = match fs::symlink_metadata(&self.path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            result => result.map_err(|_| unavailable())?,
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > BYTE_LIMIT as u64
        {
            return Err(unavailable());
        }
        let mut bytes = Vec::new();
        fs::File::open(&self.path)
            .map_err(|_| unavailable())?
            .take(BYTE_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| unavailable())?;
        if bytes.len() > BYTE_LIMIT {
            return Err(unavailable());
        }
        let ledger: Ledger = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
        if ledger.schema_version != 1 {
            return Err(unavailable());
        }
        validate(&ledger.records)?;
        Ok(ledger.records)
    }
    pub(crate) fn save(&self, records: &[CaptureRecord]) -> AppResult<()> {
        let _guard = self.publication.lock().map_err(|_| unavailable())?;
        self.write(records)
    }
    /// The existing debug worker may checkpoint outside the capture mutex.
    /// Older checkpoints never overwrite newer start/stop publications.
    pub(crate) fn save_ordered(&self, sequence: u64, records: &[CaptureRecord]) -> AppResult<bool> {
        let mut published = self.publication.lock().map_err(|_| unavailable())?;
        if sequence <= *published {
            return Ok(false);
        }
        self.write(records)?;
        *published = sequence;
        Ok(true)
    }
    fn write(&self, records: &[CaptureRecord]) -> AppResult<()> {
        validate(records)?;
        let bytes = serde_json::to_vec(&Ledger {
            schema_version: 1,
            records: records.to_vec(),
        })
        .map_err(|_| unavailable())?;
        if bytes.len() > BYTE_LIMIT {
            return Err(unavailable());
        }
        super::atomic_file::write(&self.path, &bytes).map_err(|_| unavailable())?;
        // A replacement may already be visible when durability confirmation
        // fails. Surface uncertainty rather than claiming old bytes remain.
        #[cfg(unix)]
        fs::File::open(self.path.parent().ok_or_else(unavailable)?)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| unavailable())?;
        Ok(())
    }
    /// Explicitly replaces only this owner's journal, including invalid legacy
    /// bytes. No path from the record or caller can expand the cleanup scope.
    pub(crate) fn clear(&self) -> AppResult<()> {
        self.save(&[])
    }
}

#[cfg(test)]
#[path = "capture_recovery_tests.rs"]
mod tests;
