//! Public observations omit native process tokens and never contain guest data.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginJobsSnapshot {
    pub schema_version: u32,
    pub scheduling: Option<Scheduling>,
    pub last_run: Option<Run>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scheduling {
    pub state: String,
    pub period_ms: Option<u64>,
    pub observed_at_unix_ms: Option<String>,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub run_id: String,
    pub origin: String,
    pub state: String,
    pub started_at_unix_ms: String,
    pub ended_at_unix_ms: Option<String>,
    pub observed_at_unix_ms: Option<String>,
    pub completed: Option<u32>,
    pub failed: Option<u32>,
    pub unconfirmed: Option<u32>,
    pub reason: Option<String>,
    pub result_confirmed: Option<bool>,
    pub android_stop_reason: Option<u32>,
}
fn reason(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 120
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte))
}
fn timestamp(value: &str) -> bool {
    value
        .parse::<u64>()
        .is_ok_and(|number| number.to_string() == value)
}
impl PluginJobsSnapshot {
    pub fn valid(&self) -> bool {
        self.schema_version == 1
            && self.scheduling.as_ref().is_none_or(|schedule| {
                matches!(schedule.state.as_str(), "scheduled" | "unavailable")
                    && reason(&schedule.reason)
                    && schedule.period_ms.is_none_or(|period| period == 900_000)
                    && schedule
                        .observed_at_unix_ms
                        .as_deref()
                        .is_none_or(timestamp)
            })
            && self.last_run.as_ref().is_none_or(|run| {
                run.run_id.len() == 32
                    && run
                        .run_id
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    && run.origin == "android-job-scheduler"
                    && matches!(
                        run.state.as_str(),
                        "running"
                            | "completed"
                            | "deferred"
                            | "failed"
                            | "stopped"
                            | "result-unknown"
                    )
                    && timestamp(&run.started_at_unix_ms)
                    && run.ended_at_unix_ms.as_deref().is_none_or(timestamp)
                    && run.observed_at_unix_ms.as_deref().is_none_or(timestamp)
                    && run.reason.as_deref().is_none_or(reason)
                    && [run.completed, run.failed, run.unconfirmed]
                        .into_iter()
                        .all(|value| value.is_none_or(|value| value <= 8))
                    && [run.completed, run.failed, run.unconfirmed]
                        .into_iter()
                        .flatten()
                        .sum::<u32>()
                        <= 8
                    && (!matches!(run.state.as_str(), "running" | "stopped" | "result-unknown")
                        || run.result_confirmed != Some(true))
            })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stopped_observation_preserves_unknown_counts_and_hides_native_identity() {
        let input = r#"{"schemaVersion":1,"lastRun":{"runId":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","processToken":"private","origin":"android-job-scheduler","state":"stopped","startedAtUnixMs":"1","reason":"android-system-stopped-job","resultConfirmed":false}}"#;
        let value: PluginJobsSnapshot = serde_json::from_str(input).unwrap();
        assert!(value.valid());
        assert_eq!(value.last_run.as_ref().unwrap().completed, None);
        let output = serde_json::to_string(&value).unwrap();
        assert!(!output.contains("processToken"));
        assert!(!output.contains("private"));
    }
    #[test]
    fn unconfirmed_run_cannot_claim_success() {
        let mut value: PluginJobsSnapshot = serde_json::from_str(r#"{"schemaVersion":1,"lastRun":{"runId":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","origin":"android-job-scheduler","state":"result-unknown","startedAtUnixMs":"1","resultConfirmed":true}}"#).unwrap();
        assert!(!value.valid());
        value.last_run.as_mut().unwrap().result_confirmed = Some(false);
        assert!(value.valid());
    }
}
