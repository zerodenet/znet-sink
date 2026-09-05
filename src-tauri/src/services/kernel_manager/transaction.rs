use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};
use crate::services::atomic_file;

#[derive(Serialize, Deserialize)]
struct BackupEntry {
    name: String,
    existed: bool,
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    target: PathBuf,
    state: String,
    owner_pid: u32,
    entries: Vec<BackupEntry>,
}

/// A durable copy of every file touched by an upgrade. Kept on disk after
/// success and after a failed rollback so recovery never depends on staging.
pub struct BundleTransaction {
    backup: PathBuf,
    receipt: Receipt,
}

impl BundleTransaction {
    pub fn prepare(target: &Path, backup_root: &Path, names: &[String]) -> AppResult<Self> {
        fs::create_dir_all(backup_root).map_err(storage_error)?;
        let workspace = tempfile::Builder::new()
            .prefix("upgrade-")
            .tempdir_in(backup_root)
            .map_err(storage_error)?;
        let mut entries = Vec::new();
        for name in names.iter().collect::<BTreeSet<_>>() {
            if !super::safe_runtime_file_name(name) {
                return Err(AppError::invalid_argument(
                    "invalid kernel backup file name",
                ));
            }
            let path = target.join(name);
            let existed = match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => true,
                Ok(_) => return Err(super::runtime_file_conflict(&path)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) => return Err(storage_error(error)),
            };
            if existed {
                atomic_file::copy(&path, &workspace.path().join("files").join(name))
                    .map_err(storage_error)?;
            }
            entries.push(BackupEntry {
                name: name.clone(),
                existed,
            });
        }
        let receipt = Receipt {
            target: target.to_owned(),
            state: "pending".into(),
            owner_pid: std::process::id(),
            entries,
        };
        write_receipt(workspace.path(), &receipt)?;
        Ok(Self {
            backup: workspace.keep(),
            receipt,
        })
    }

    pub fn backup_path(&self) -> &Path {
        &self.backup
    }

    pub fn preserve_config(&self, config: &crate::models::app_config::AppConfig) -> AppResult<()> {
        let bytes = serde_json::to_vec_pretty(config)
            .map_err(|error| AppError::internal(error.to_string()))?;
        atomic_file::write(&self.backup.join("previous-app-config.json"), &bytes)
            .map_err(storage_error)
    }

    pub fn commit(&mut self) -> AppResult<()> {
        self.receipt.state = "committed".into();
        write_receipt(&self.backup, &self.receipt)
    }

    pub fn rollback(&mut self) -> AppResult<()> {
        let mut failures = Vec::new();
        for entry in &self.receipt.entries {
            let target = self.receipt.target.join(&entry.name);
            let result = if entry.existed {
                let source = self.backup.join("files").join(&entry.name);
                replace_file(&source, &target)
            } else {
                match fs::remove_file(&target) {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(error) => Err(storage_error(error)),
                }
            };
            if let Err(error) = result {
                failures.push(format!("{}: {}", entry.name, error.message));
            }
        }
        if !failures.is_empty() {
            return Err(AppError::internal(format!(
                "kernel rollback incomplete; backup retained at {}: {}",
                self.backup.display(),
                failures.join("; ")
            )));
        }
        self.receipt.state = "rolled_back".into();
        write_receipt(&self.backup, &self.receipt)
    }
}

pub(super) fn replace_file(source: &Path, target: &Path) -> AppResult<()> {
    let mut last_error = None;
    for attempt in 0..5 {
        match atomic_file::copy(source, target) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
        if attempt < 4 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    Err(storage_error(last_error.unwrap()))
}

fn write_receipt(backup: &Path, receipt: &Receipt) -> AppResult<()> {
    let content = serde_json::to_vec_pretty(receipt)
        .map_err(|error| AppError::internal(error.to_string()))?;
    atomic_file::write(&backup.join("receipt.json"), &content).map_err(storage_error)
}

fn storage_error(error: std::io::Error) -> AppError {
    AppError::internal(format!("kernel upgrade storage failed: {error}"))
}

#[cfg(test)]
#[path = "transaction_tests.rs"]
mod tests;
