//! Metadata only: no record bodies, repair, pruning, or cleanup during inspection.
use crate::{errors::AppResult, state::app_state::AppState};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalStoreStorage {
    pub kind: &'static str,
    pub state: &'static str,
    pub bytes: Option<String>,
    pub files: Option<usize>,
    pub byte_limit: Option<String>,
    pub record_limit: Option<usize>,
    pub max_age_ms: Option<String>,
    pub enforcement: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalDataSummary {
    pub sampled_at_unix_ms: String,
    pub stores: Vec<LocalStoreStorage>,
}

pub(crate) fn summary(state: &AppState) -> AppResult<LocalDataSummary> {
    let log_limit = super::common::lock(state.app_config(), "app_config")?
        .logs
        .max_entries;
    Ok(LocalDataSummary {
        sampled_at_unix_ms: super::common::now_unix_ms().to_string(),
        stores: vec![
            super::log_store::storage(log_limit),
            super::connection_history_store::storage(),
            super::debug_store::storage(),
        ],
    })
}

pub(crate) fn inspect(
    kind: &'static str,
    paths: AppResult<(PathBuf, Vec<PathBuf>)>,
    byte_limit: Option<u64>,
    record_limit: Option<usize>,
    max_age_ms: Option<u64>,
    enforcement: &'static str,
) -> LocalStoreStorage {
    let observed = paths
        .ok()
        .and_then(|(root, paths)| sizes(&root, &paths).ok());
    LocalStoreStorage {
        kind,
        state: if observed.is_some() {
            "sampled"
        } else {
            "unavailable"
        },
        bytes: observed.map(|value| value.0.to_string()),
        files: observed.map(|value| value.1),
        byte_limit: byte_limit.map(|value| value.to_string()),
        record_limit,
        max_age_ms: max_age_ms.map(|value| value.to_string()),
        enforcement,
    }
}

fn sizes(root: &Path, paths: &[PathBuf]) -> Result<(u64, usize), ()> {
    let mut total = 0u64;
    let mut files = 0;
    for path in paths {
        let relative = path.strip_prefix(root).map_err(|_| ())?;
        let mut cursor = root.to_path_buf();
        let mut missing = false;
        for component in relative.components() {
            if !matches!(component, std::path::Component::Normal(_)) {
                return Err(());
            }
            cursor.push(component);
            match fs::symlink_metadata(&cursor) {
                Ok(metadata) if metadata.file_type().is_symlink() => return Err(()),
                Ok(metadata) if cursor == *path && metadata.is_file() => {
                    total = total.checked_add(metadata.len()).ok_or(())?;
                    files += 1;
                }
                Ok(metadata) if cursor != *path && metadata.is_dir() => {}
                Ok(_) => return Err(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    missing = true;
                    break;
                }
                Err(_) => return Err(()),
            }
        }
        if !missing && cursor != *path {
            return Err(());
        }
    }
    Ok((total, files))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_distinguishes_empty_real_bytes_and_unreadable_without_reading_bodies() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        fs::create_dir_all(root.join("logs")).unwrap();
        let file = root.join("logs/owned");
        assert_eq!(sizes(&root, std::slice::from_ref(&file)), Ok((0, 0)));
        fs::write(&file, b"secret-body-not-returned").unwrap();
        assert_eq!(sizes(&root, std::slice::from_ref(&file)), Ok((24, 1)));
        let value = inspect(
            "logs",
            Ok((root.clone(), vec![file])),
            None,
            Some(5000),
            None,
            "append-rotation",
        );
        let serialized = serde_json::to_string(&value).unwrap();
        assert!(!serialized.contains("secret") && !serialized.contains(root.to_str().unwrap()));
        let blocked = inspect(
            "logs",
            Ok((root.clone(), vec![root.join("logs")])),
            None,
            None,
            None,
            "append-rotation",
        );
        assert_eq!(blocked.state, "unavailable");
        assert_eq!(blocked.bytes, None);
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn symlink_ancestors_and_nonregular_paths_never_report_a_successful_zero() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        fs::create_dir_all(root.join("outside")).unwrap();
        std::os::unix::fs::symlink(root.join("outside"), root.join("logs")).unwrap();
        assert!(sizes(&root, &[root.join("logs/absent")]).is_err());
        assert!(sizes(&root, &[root.join("../outside")]).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
