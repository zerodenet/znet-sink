//! Client-side node latency probing.
//!
//! Orchestrates speed tests (queue, concurrency, progress) on the client side.
//! Individual node probes go through the core engine's outbound IPC probe without any
//! upfront health check — each probe handles its own timeout and failure.

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use tauri::{AppHandle, Emitter, Manager};

use crate::client_core::{
    ProbeJobId, ProbeJobKind, ProbeJobSnapshot, ProbeJobState, ProbeObservation,
    ProbeObservationSource, ProbeTargetResult,
};
use crate::errors::AppError;
use crate::kernel::zero::commands;
use crate::models::gui_core::{GuiPolicyGroup, GuiPolicyProbeCompletedEvent};
use crate::models::logs::LogLevel;
use crate::services::{common, core_config, logs};
use crate::state::app_state::AppState;

/// Maximum concurrent probe requests to the core.
pub const MAX_CONCURRENT_PROBES: usize = 8;
pub const PROBE_JOB_UPDATED_EVENT: &str = "client-core:probe-job-updated";
pub const CLIENT_CORE_UPDATED_EVENT: &str = "client-core:updated";

static PROBE_CONCURRENCY: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();

fn probe_semaphore() -> Arc<tokio::sync::Semaphore> {
    PROBE_CONCURRENCY
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_PROBES)))
        .clone()
}

/// Per-node probe result.
#[derive(Clone, Debug)]
pub struct ProbeResult {
    pub target_tag: String,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub message: Option<String>,
}

/// Probe a single node through the core's full outbound proxy stack.
/// No upfront health check — the probe itself handles timeout/failure.
pub async fn probe_single(state: &AppState, job_id: ProbeJobId, target_tag: &str) -> ProbeResult {
    let target_tag = target_tag.trim().to_string();
    let requested_at_unix_ms = common::now_unix_ms();
    let started = Instant::now();
    log_probe_request(state, job_id, &target_tag, requested_at_unix_ms);

    if target_tag.is_empty() {
        let result = ProbeResult {
            target_tag: target_tag.clone(),
            reachable: false,
            latency_ms: None,
            message: Some("target tag must not be empty".to_string()),
        };
        log_probe_response(
            state,
            job_id,
            &result,
            requested_at_unix_ms,
            started.elapsed().as_millis() as u64,
            Some("invalid_argument"),
            None,
        );
        return result;
    }

    // Build IPC options from app config.
    let options = match default_ipc_options(state) {
        Ok(opts) => opts,
        Err(error) => {
            let result = ProbeResult {
                target_tag,
                reachable: false,
                latency_ms: None,
                message: Some(format!("IPC config error: {}", error.message)),
            };
            log_probe_response(
                state,
                job_id,
                &result,
                requested_at_unix_ms,
                started.elapsed().as_millis() as u64,
                Some(error.code),
                error.details.as_ref(),
            );
            return result;
        }
    };

    // Persist the normalized Zero response at the Rust boundary. This remains
    // observable even when the node page is closed or misses a Tauri event.
    match commands::probe_outbound(target_tag.clone(), None, options).await {
        Ok(response) => {
            let result = ProbeResult {
                target_tag: response.target_tag,
                reachable: response.reachable,
                latency_ms: response.latency_ms,
                message: response.message,
            };
            log_probe_response(
                state,
                job_id,
                &result,
                requested_at_unix_ms,
                started.elapsed().as_millis() as u64,
                None,
                None,
            );
            result
        }
        Err(error) => {
            let result = ProbeResult {
                target_tag,
                reachable: false,
                latency_ms: None,
                message: Some(error.message.clone()),
            };
            log_probe_response(
                state,
                job_id,
                &result,
                requested_at_unix_ms,
                started.elapsed().as_millis() as u64,
                Some(error.code),
                error.details.as_ref(),
            );
            result
        }
    }
}

fn log_probe_request(
    state: &AppState,
    job_id: ProbeJobId,
    target_tag: &str,
    requested_at_unix_ms: u64,
) {
    logs::znet_log_fields(
        Some(state),
        LogLevel::Debug,
        format!("节点测速内核请求（{target_tag}）"),
        serde_json::json!({
            "schema": "znet.node-probe.v1",
            "area": "nodes",
            "operation": "probe.request",
            "method": "diagnostics.probe_outbound",
            "probeJobId": job_id.0,
            "targetTag": target_tag,
            "requestedAtUnixMs": requested_at_unix_ms,
        }),
    );
}

fn log_probe_response(
    state: &AppState,
    job_id: ProbeJobId,
    result: &ProbeResult,
    requested_at_unix_ms: u64,
    duration_ms: u64,
    error_code: Option<&str>,
    error_details: Option<&serde_json::Value>,
) {
    let responded_at_unix_ms = common::now_unix_ms();
    let detail = if result.reachable {
        result
            .latency_ms
            .map(|latency| format!("{latency} ms"))
            .unwrap_or_else(|| "reachable".to_string())
    } else {
        result
            .message
            .clone()
            .unwrap_or_else(|| "unreachable".to_string())
    };
    logs::znet_log_fields(
        Some(state),
        if result.reachable {
            LogLevel::Info
        } else {
            LogLevel::Warn
        },
        format!("节点测速内核响应（{}）：{detail}", result.target_tag),
        serde_json::json!({
            "schema": "znet.node-probe.v1",
            "area": "nodes",
            "operation": "probe.response",
            "method": "diagnostics.probe_outbound",
            "probeJobId": job_id.0,
            "targetTag": result.target_tag,
            "requestedAtUnixMs": requested_at_unix_ms,
            "respondedAtUnixMs": responded_at_unix_ms,
            "durationMs": duration_ms,
            "reachable": result.reachable,
            "latencyMs": result.latency_ms,
            "kernelMessage": result.message,
            "errorCode": error_code,
            "errorDetails": error_details,
            "outcome": if result.reachable { "success" } else { "failed" },
        }),
    );
}

/// Execute a Client Core-owned probe job. Tauri is only used to schedule work
/// and publish advisory updates; the job remains recoverable from AppState.
pub async fn run_probe_job(app_handle: AppHandle, job: ProbeJobSnapshot) {
    match job.kind {
        ProbeJobKind::Outbound => run_outbound_probe_job(app_handle, job).await,
        ProbeJobKind::ManualPolicy => run_policy_probe_job(app_handle, job).await,
        ProbeJobKind::ScheduledPolicyObservation => {}
    }
}

async fn run_outbound_probe_job(app_handle: AppHandle, job: ProbeJobSnapshot) {
    let mut handles = Vec::with_capacity(job.target_tags.len());

    for target_tag in job.target_tags.clone() {
        let app = app_handle.clone();
        let scope = job.scope.clone();
        let job_id = job.id;
        handles.push(tauri::async_runtime::spawn(async move {
            let Ok(_permit) = probe_semaphore().acquire_owned().await else {
                return;
            };
            let state = app.state::<AppState>();
            if state
                .get_client_probe_job(job_id)
                .is_none_or(|current| current.state != ProbeJobState::Running)
            {
                return;
            }
            let result = probe_single(state.inner(), job_id, &target_tag).await;
            let update = state.record_client_probe_result(
                job_id,
                &scope,
                ProbeTargetResult {
                    target_tag: result.target_tag,
                    reachable: result.reachable,
                    latency_ms: result.latency_ms,
                    message: result.message,
                    source: ProbeObservationSource::ManualOutbound,
                    observed_at_unix_ms: common::now_unix_ms(),
                },
            );
            if let Some(update) = update {
                let _ = app.emit(PROBE_JOB_UPDATED_EVENT, update);
            }
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }
}

async fn run_policy_probe_job(app_handle: AppHandle, job: ProbeJobSnapshot) {
    for policy_tag in job.target_tags.clone() {
        let Ok(_permit) = probe_semaphore().acquire_owned().await else {
            return;
        };
        let state = app_handle.state::<AppState>();
        if state
            .get_client_probe_job(job.id)
            .is_none_or(|current| current.state != ProbeJobState::Running)
        {
            return;
        }
        let options = match default_ipc_options(state.inner()) {
            Ok(options) => options,
            Err(error) => {
                record_policy_job_failure(&app_handle, &job, policy_tag, error.message);
                continue;
            }
        };
        let command = commands::probe_policy(policy_tag.clone(), options).await;
        let rejection = match command {
            Err(error) => Some(error.message),
            Ok(response) if !commands::policy_probe_command_accepted(&response) => {
                Some("kernel rejected the policy probe request".to_string())
            }
            Ok(_) => None,
        };
        if let Some(message) = rejection {
            record_policy_job_failure(&app_handle, &job, policy_tag, message);
        } else {
            wait_for_policy_target(&app_handle, job.id, &policy_tag).await;
        }
    }
}

async fn wait_for_policy_target(
    app_handle: &AppHandle,
    job_id: crate::client_core::ProbeJobId,
    target: &str,
) {
    loop {
        let state = app_handle.state::<AppState>();
        let Some(job) = state.get_client_probe_job(job_id) else {
            return;
        };
        if job.state.is_terminal() || job.results.iter().any(|result| result.target_tag == target) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

fn record_policy_job_failure(
    app_handle: &AppHandle,
    job: &ProbeJobSnapshot,
    policy_tag: String,
    message: String,
) {
    let state = app_handle.state::<AppState>();
    let update = state.record_client_probe_result(
        job.id,
        &job.scope,
        ProbeTargetResult {
            target_tag: policy_tag,
            reachable: false,
            latency_ms: None,
            message: Some(message),
            source: ProbeObservationSource::ManualPolicy,
            observed_at_unix_ms: common::now_unix_ms(),
        },
    );
    if let Some(update) = update {
        let _ = app_handle.emit(PROBE_JOB_UPDATED_EVENT, update);
    }
}

/// Reconcile a normalized Zero policy completion event into any matching
/// manual policy job. Scheduled observations remain distinguishable and do
/// not masquerade as completion of an overlapping manual request.
pub fn record_policy_probe_completed(app_handle: &AppHandle, event: &GuiPolicyProbeCompletedEvent) {
    let scheduled =
        crate::kernel::zero::events::policy_probe_is_scheduled(event.trigger.as_deref());

    let state = app_handle.state::<AppState>();
    let current_scope = state.client_core_snapshot().scope;
    let observation_source = if scheduled {
        ProbeObservationSource::ScheduledPolicy
    } else {
        ProbeObservationSource::ManualPolicy
    };
    let observation_kind = if scheduled {
        ProbeJobKind::ScheduledPolicyObservation
    } else {
        ProbeJobKind::ManualPolicy
    };
    let observed_at_unix_ms = event
        .completed_at_unix_ms
        .unwrap_or_else(common::now_unix_ms);
    for member in &event.members {
        state.record_client_probe_observation(ProbeObservation {
            scope: current_scope.clone(),
            job_kind: observation_kind,
            target_tag: member.tag.clone(),
            reachable: member.alive.unwrap_or(member.last_error.is_none()),
            latency_ms: member.delay_ms,
            message: member.last_error.clone(),
            source: observation_source,
            observed_at_unix_ms: member.last_checked_unix_ms.unwrap_or(observed_at_unix_ms),
            selected_tag: event.selected.clone(),
        });
    }
    let _ = app_handle.emit(CLIENT_CORE_UPDATED_EVENT, state.client_core_snapshot());
    if scheduled {
        return;
    }

    let matching_jobs: Vec<_> = state
        .list_client_probe_jobs(None)
        .into_iter()
        .filter(|job| {
            job.state == ProbeJobState::Running
                && job.kind == ProbeJobKind::ManualPolicy
                && job.scope == current_scope
                && job.target_tags.contains(&event.policy_tag)
                && policy_completion_is_fresh(event, job)
        })
        .collect();
    if matching_jobs.is_empty() {
        return;
    }

    let failed = event
        .members
        .iter()
        .filter(|member| member.alive == Some(false) || member.last_error.is_some())
        .count();
    let latency_ms = event
        .selected
        .as_deref()
        .and_then(|selected| event.members.iter().find(|member| member.tag == selected))
        .and_then(|member| member.delay_ms)
        .or_else(|| {
            event
                .members
                .iter()
                .filter_map(|member| member.delay_ms)
                .min()
        });
    for job in matching_jobs {
        let update = state.record_client_probe_result(
            job.id,
            &job.scope,
            ProbeTargetResult {
                target_tag: event.policy_tag.clone(),
                reachable: failed == 0 && !event.members.is_empty(),
                latency_ms,
                message: (failed > 0)
                    .then(|| format!("{failed}/{} policy members failed", event.members.len())),
                source: ProbeObservationSource::ManualPolicy,
                observed_at_unix_ms,
            },
        );
        if let Some(update) = update {
            let _ = app_handle.emit(PROBE_JOB_UPDATED_EVENT, update);
        }
    }
}

fn policy_completion_is_fresh(
    event: &GuiPolicyProbeCompletedEvent,
    job: &ProbeJobSnapshot,
) -> bool {
    if event
        .started_at_unix_ms
        .is_some_and(|started| started < job.started_at_unix_ms)
        || event
            .completed_at_unix_ms
            .is_some_and(|completed| completed < job.started_at_unix_ms)
    {
        return false;
    }
    let latest_member_check = event
        .members
        .iter()
        .filter_map(|member| member.last_checked_unix_ms)
        .max();
    latest_member_check.is_none_or(|checked| checked >= job.started_at_unix_ms)
}

/// Recover manual policy jobs after an event gap by comparing the latest
/// kernel policy snapshot with each job's request time.
pub fn reconcile_policy_snapshot(app_handle: &AppHandle, groups: &[GuiPolicyGroup]) {
    let state = app_handle.state::<AppState>();
    let current_scope = state.client_core_snapshot().scope;
    let jobs: Vec<_> = state
        .list_client_probe_jobs(None)
        .into_iter()
        .filter(|job| {
            job.state == ProbeJobState::Running
                && job.kind == ProbeJobKind::ManualPolicy
                && job.scope == current_scope
        })
        .collect();

    for job in jobs {
        for target in &job.target_tags {
            let Some(group) = groups.iter().find(|group| group.name == *target) else {
                continue;
            };
            let completed_at_unix_ms = group
                .outbounds
                .iter()
                .filter_map(|member| member.last_checked_unix_ms)
                .max();
            if completed_at_unix_ms.is_none_or(|completed| completed < job.started_at_unix_ms) {
                continue;
            }
            record_policy_probe_completed(
                app_handle,
                &GuiPolicyProbeCompletedEvent {
                    policy_tag: group.name.clone(),
                    trigger: Some("manual".to_string()),
                    url: None,
                    started_at_unix_ms: Some(job.started_at_unix_ms),
                    completed_at_unix_ms,
                    duration_ms: completed_at_unix_ms
                        .map(|completed| completed.saturating_sub(job.started_at_unix_ms)),
                    selected: group.selected.clone(),
                    members: group.outbounds.clone(),
                },
            );
        }
    }
}

pub fn spawn_probe_timeout(app_handle: AppHandle, job: &ProbeJobSnapshot) {
    let wait_ms = job
        .deadline_at_unix_ms
        .saturating_sub(common::now_unix_ms());
    let job_id = job.id;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
        let state = app_handle.state::<AppState>();
        if let Some(update) = state.timeout_client_probe(job_id) {
            if update.state == ProbeJobState::TimedOut {
                let _ = app_handle.emit(PROBE_JOB_UPDATED_EVENT, update);
            }
        }
    });
}

fn default_ipc_options(
    state: &AppState,
) -> Result<Option<crate::models::core::CoreIpcOptions>, AppError> {
    let config = common::lock(state.app_config(), "app_config")?.core.clone();
    Ok(Some(core_config::ipc_options_from_app_config(&config)))
}

#[cfg(test)]
mod tests {
    use super::policy_completion_is_fresh;
    use crate::client_core::{
        ClientScope, ConfigRevision, CoreInstanceId, ProbeJobId, ProbeJobKind, ProbeJobSnapshot,
        ProbeJobState, ProfileId,
    };
    use crate::models::gui_core::{GuiPolicyMember, GuiPolicyProbeCompletedEvent};

    fn job(started_at: u64) -> ProbeJobSnapshot {
        ProbeJobSnapshot {
            id: ProbeJobId(1),
            scope: ClientScope {
                profile_id: Some(ProfileId("profile-a".to_string())),
                config_revision: ConfigRevision(10),
                core_instance_id: CoreInstanceId(2),
            },
            kind: ProbeJobKind::ManualPolicy,
            state: ProbeJobState::Running,
            target_tags: vec!["auto".to_string()],
            results: Vec::new(),
            completed: 0,
            succeeded: 0,
            failed: 0,
            started_at_unix_ms: started_at,
            updated_at_unix_ms: started_at,
            deadline_at_unix_ms: started_at + 30_000,
        }
    }

    fn event(
        started: Option<u64>,
        completed: Option<u64>,
        checked: Option<u64>,
    ) -> GuiPolicyProbeCompletedEvent {
        GuiPolicyProbeCompletedEvent {
            policy_tag: "auto".to_string(),
            trigger: Some("manual".to_string()),
            url: None,
            started_at_unix_ms: started,
            completed_at_unix_ms: completed,
            duration_ms: None,
            selected: Some("node-a".to_string()),
            members: vec![GuiPolicyMember {
                tag: "node-a".to_string(),
                kind: None,
                selected: true,
                alive: Some(true),
                delay_ms: Some(20),
                last_checked_unix_ms: checked,
                last_error: None,
            }],
        }
    }

    #[test]
    fn delayed_completion_from_before_job_start_is_rejected() {
        let job = job(1_000);
        assert!(!policy_completion_is_fresh(
            &event(Some(800), Some(900), Some(900)),
            &job
        ));
        assert!(!policy_completion_is_fresh(
            &event(None, None, Some(900)),
            &job
        ));
        assert!(policy_completion_is_fresh(
            &event(Some(1_000), Some(1_100), Some(1_050)),
            &job
        ));
    }

    #[test]
    fn timestamp_free_legacy_completion_has_deterministic_compatibility_fallback() {
        assert!(policy_completion_is_fresh(
            &event(None, None, None),
            &job(1_000)
        ));
    }
}
