use std::fs;
use std::path::Path;

use crate::client_core::ProbeObservation;
use crate::errors::{AppError, AppResult};

const PROBE_HISTORY_FILE: &str = "probe-history.json";

pub(crate) fn load() -> AppResult<Vec<ProbeObservation>> {
    load_from_dir(&super::data_dir()?)
}

pub(crate) fn save(observations: &[ProbeObservation]) -> AppResult<()> {
    save_to_dir(&super::data_dir()?, observations)
}

fn load_from_dir(dir: &Path) -> AppResult<Vec<ProbeObservation>> {
    let path = dir.join(PROBE_HISTORY_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to read probe history: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })?;
    serde_json::from_str(&content).map_err(|error| AppError {
        code: "invalid_argument",
        message: format!("failed to parse probe history: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })
}

fn save_to_dir(dir: &Path, observations: &[ProbeObservation]) -> AppResult<()> {
    fs::create_dir_all(dir).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to create probe history directory: {error}"),
        details: Some(serde_json::json!({ "path": dir.display().to_string() })),
    })?;
    let path = dir.join(PROBE_HISTORY_FILE);
    let content = serde_json::to_string_pretty(observations).map_err(|error| {
        AppError::internal(format!("failed to serialize probe history: {error}"))
    })?;
    fs::write(&path, content).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to write probe history: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })
}

#[cfg(test)]
mod tests {
    use super::{load_from_dir, save_to_dir};
    use crate::client_core::{
        ClientScope, ConfigRevision, CoreInstanceId, ProbeJobKind, ProbeObservation,
        ProbeObservationSource, ProfileId,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "znet-sink-probe-history-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn observation(profile: &str, revision: u64, tag: &str) -> ProbeObservation {
        ProbeObservation {
            scope: ClientScope {
                profile_id: Some(ProfileId(profile.to_string())),
                config_revision: ConfigRevision(revision),
                core_instance_id: CoreInstanceId(3),
            },
            job_kind: ProbeJobKind::ManualPolicy,
            target_tag: tag.to_string(),
            reachable: true,
            latency_ms: Some(42),
            message: None,
            source: ProbeObservationSource::ManualPolicy,
            observed_at_unix_ms: 1_000,
            policy_tag: Some(tag.to_string()),
            selected_tag: Some(tag.to_string()),
        }
    }

    #[test]
    fn scoped_history_round_trips_without_losing_identity_or_source() {
        let dir = temp_dir("roundtrip");
        let expected = vec![
            observation("profile-a", 10, "shared"),
            observation("profile-b", 20, "shared"),
        ];

        save_to_dir(&dir, &expected).unwrap();
        assert_eq!(load_from_dir(&dir).unwrap(), expected);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_history_file_loads_as_empty() {
        let dir = temp_dir("missing");
        assert!(load_from_dir(&dir).unwrap().is_empty());
    }
}
