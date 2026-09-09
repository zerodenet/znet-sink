use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use tauri::{AppHandle, Emitter, Manager};

use crate::client_core::{
    ProbeJobId, ProbeJobKind, ProbeJobSnapshot, ProbeJobState, ProbeObservationSource,
    ProbeTargetResult, StartProbeRequest,
};
use crate::errors::AppError;
use crate::kernel::adapter::KernelAdapter;
use crate::kernel::zero::{commands, ZeroAdapter};
use crate::models::gui_core::GuiPolicyProbeCompletedEvent;
use crate::models::logs::LogLevel;
use crate::services::{common, core_config, logs};
use crate::state::app_state::AppState;

use super::policy_probe_summary;

/// Maximum concurrent probe requests to the core.
pub const MAX_CONCURRENT_PROBES: usize = 8;
pub const PROBE_JOB_UPDATED_EVENT: &str = "client-core:probe-job-updated";

static PROBE_CONCURRENCY: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
static POLICY_PROBE_OPERATIONS: OnceLock<Mutex<HashMap<(ProbeJobId, String), String>>> =
    OnceLock::new();

fn policy_probe_operations() -> &'static Mutex<HashMap<(ProbeJobId, String), String>> {
    POLICY_PROBE_OPERATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn remember_policy_probe_operation(job_id: ProbeJobId, policy_tag: &str, operation_id: String) {
    policy_probe_operations()
        .lock()
        .expect("policy probe operation lock poisoned")
        .insert((job_id, policy_tag.to_owned()), operation_id);
}

fn expected_policy_probe_operation(job_id: ProbeJobId, policy_tag: &str) -> Option<String> {
    policy_probe_operations()
        .lock()
        .expect("policy probe operation lock poisoned")
        .get(&(job_id, policy_tag.to_owned()))
        .cloned()
}

fn forget_policy_probe_operation(job_id: ProbeJobId, policy_tag: &str) {
    policy_probe_operations()
        .lock()
        .expect("policy probe operation lock poisoned")
        .remove(&(job_id, policy_tag.to_owned()));
}

pub(crate) fn forget_policy_probe_job(job_id: ProbeJobId) {
    policy_probe_operations()
        .lock()
        .expect("policy probe operation lock poisoned")
        .retain(|(registered_job_id, _), _| *registered_job_id != job_id);
}

fn probe_semaphore() -> Arc<tokio::sync::Semaphore> {
    PROBE_CONCURRENCY
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_PROBES)))
        .clone()
}

const MIN_POLICY_PROBE_TIMEOUT_MS: u64 = 60_000;
const POLICY_PROBE_BASE_TIMEOUT_MS: u64 = 15_000;
const POLICY_PROBE_MEMBER_TIMEOUT_MS: u64 = 10_000;
const MAX_POLICY_PROBE_TIMEOUT_MS: u64 = 10 * 60_000;

fn policy_probe_timeout_ms(member_count: usize) -> u64 {
    POLICY_PROBE_BASE_TIMEOUT_MS
        .saturating_add((member_count.max(1) as u64).saturating_mul(POLICY_PROBE_MEMBER_TIMEOUT_MS))
        .clamp(MIN_POLICY_PROBE_TIMEOUT_MS, MAX_POLICY_PROBE_TIMEOUT_MS)
}

/// Apply a backend-owned policy deadline derived from the active configuration.
/// A manual click can overlap an in-flight scheduled cycle, and older kernels
/// may effectively probe members serially, so five seconds per member is not a
/// safe client deadline. Explicit callers may request a longer budget, but not
/// shorten this compatibility floor.
pub fn normalize_start_request(
    state: &AppState,
    mut request: StartProbeRequest,
) -> Result<StartProbeRequest, AppError> {
    if request.kind != ProbeJobKind::ManualPolicy {
        return Ok(request);
    }

    let active_content = common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .and_then(|profile| profile.content.clone());
    let adapter = ZeroAdapter::new();
    let groups = active_content
        .as_ref()
        .map(|content| adapter.policy_groups_from_config(content))
        .transpose()?
        .unwrap_or_default();
    let member_count = request
        .target_tags
        .iter()
        .map(|target| {
            groups
                .iter()
                .find(|group| group.name == *target)
                .map(|group| group.outbounds.len())
                .unwrap_or(1)
        })
        .sum::<usize>()
        .max(1);
    let timeout_ms = policy_probe_timeout_ms(member_count);
    request.timeout_ms = Some(request.timeout_ms.unwrap_or_default().max(timeout_ms));
    Ok(request)
}

/// Per-node probe result.
#[derive(Clone, Debug)]
pub struct ProbeResult {
    pub target_tag: String,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    /// Stable business-facing message used by Client Core and the node page.
    pub message: Option<String>,
    /// Exact message returned by Zero for diagnostics. Never use this as UI copy.
    pub kernel_message: Option<String>,
    pub client_error_code: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OutboundProbeFailure {
    client_error_code: String,
    client_message: String,
}

/// Normalize legacy kernel implementation details at the client boundary while
/// retaining the exact kernel message separately for raw diagnostics.
fn normalize_outbound_probe_failure(
    raw_message: Option<&str>,
    fallback_error_code: Option<&str>,
) -> OutboundProbeFailure {
    let raw_message = raw_message
        .map(str::trim)
        .filter(|message| !message.is_empty());
    let timed_out = raw_message
        .is_some_and(|message| message.to_ascii_lowercase().contains("timed out"))
        || fallback_error_code.is_some_and(|code| {
            matches!(
                code.trim().to_ascii_lowercase().as_str(),
                "timeout" | "timed_out" | "deadline_exceeded"
            )
        });

    if timed_out {
        return OutboundProbeFailure {
            client_error_code: "probe_timeout".to_string(),
            client_message: "节点延迟测速超时".to_string(),
        };
    }

    OutboundProbeFailure {
        client_error_code: fallback_error_code.unwrap_or("probe_failed").to_string(),
        client_message: raw_message.unwrap_or("节点延迟测速失败").to_string(),
    }
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
            kernel_message: None,
            client_error_code: Some("invalid_argument".to_string()),
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
    let (options, url) = match default_ipc_options(state).and_then(|options| {
        crate::services::url_test::configured_url(state).map(|url| (options, url))
    }) {
        Ok(opts) => opts,
        Err(error) => {
            let result = ProbeResult {
                target_tag,
                reachable: false,
                latency_ms: None,
                message: Some(format!("IPC config error: {}", error.message)),
                kernel_message: None,
                client_error_code: Some(error.code.to_string()),
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
    match commands::probe_outbound(target_tag.clone(), Some(url), options).await {
        Ok(response) => {
            let kernel_message = response.message;
            let normalized_failure = (!response.reachable)
                .then(|| normalize_outbound_probe_failure(kernel_message.as_deref(), None));
            let result = ProbeResult {
                target_tag: response.target_tag,
                reachable: response.reachable,
                latency_ms: response.latency_ms,
                message: normalized_failure
                    .as_ref()
                    .map(|failure| failure.client_message.clone()),
                kernel_message,
                client_error_code: normalized_failure.map(|failure| failure.client_error_code),
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
            let normalized_failure =
                normalize_outbound_probe_failure(Some(error.message.as_str()), Some(error.code));
            let result = ProbeResult {
                target_tag,
                reachable: false,
                latency_ms: None,
                message: Some(normalized_failure.client_message),
                kernel_message: None,
                client_error_code: Some(normalized_failure.client_error_code),
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
        format!("应用向内核发送节点测速请求（{target_tag}）"),
        serde_json::json!({
            "schema": "znet.node-probe.v1",
            "area": "nodes",
            "operation": "probe.request",
            "method": "diagnostics.probe_outbound",
            "probeKind": "outbound",
            "affectsPolicySelection": false,
            "observer": "znet-sink",
            "peer": "zero-core",
            "direction": "request",
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
        format!(
            "节点延迟测速{}（{}）：{detail}",
            if result.reachable { "完成" } else { "失败" },
            result.target_tag
        ),
        serde_json::json!({
            "schema": "znet.node-probe.v1",
            "area": "nodes",
            "operation": "probe.response",
            "method": "diagnostics.probe_outbound",
            "probeKind": "outbound",
            "affectsPolicySelection": false,
            "observer": "znet-sink",
            "peer": "zero-core",
            "direction": "response",
            "probeJobId": job_id.0,
            "targetTag": result.target_tag,
            "requestedAtUnixMs": requested_at_unix_ms,
            "respondedAtUnixMs": responded_at_unix_ms,
            "durationMs": duration_ms,
            "reachable": result.reachable,
            "latencyMs": result.latency_ms,
            "clientErrorCode": result.client_error_code.as_deref(),
            "clientMessage": result.message.as_deref(),
            "kernelMessage": result.kernel_message.as_deref(),
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
        let requested_operation_id = format!("gui-manual-policy-{}", job.id.0);
        remember_policy_probe_operation(job.id, &policy_tag, requested_operation_id.clone());
        let command = commands::probe_policy_with_operation_id(
            policy_tag.clone(),
            Some(requested_operation_id),
            options,
        )
        .await;
        let rejection = match command {
            Err(error) => Some(error.message),
            Ok(response) if !commands::policy_probe_command_accepted(&response) => {
                Some("kernel rejected the policy probe request".to_string())
            }
            Ok(response) => {
                if let Some(operation_id) = commands::policy_probe_operation_id(&response) {
                    remember_policy_probe_operation(job.id, &policy_tag, operation_id.to_owned());
                }
                None
            }
        };
        if let Some(message) = rejection {
            forget_policy_probe_operation(job.id, &policy_tag);
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

fn policy_completion_matches_job(
    event: &GuiPolicyProbeCompletedEvent,
    job: &ProbeJobSnapshot,
) -> bool {
    if crate::kernel::zero::events::policy_probe_is_automatic(event.trigger.as_deref()) {
        return false;
    }
    if event
        .trigger
        .as_deref()
        .is_some_and(|trigger| !trigger.eq_ignore_ascii_case("manual"))
    {
        return false;
    }
    if let (Some(expected), Some(actual)) = (
        expected_policy_probe_operation(job.id, &event.policy_tag),
        event.operation_id.as_deref(),
    ) {
        return expected == actual;
    }
    policy_completion_is_fresh(event, job)
}

fn policy_completion_is_fresh(
    event: &GuiPolicyProbeCompletedEvent,
    job: &ProbeJobSnapshot,
) -> bool {
    if let Some(completed) = event.completed_at_unix_ms {
        return completed >= job.started_at_unix_ms;
    }
    if let Some(checked) = event
        .members
        .iter()
        .filter_map(|member| member.last_checked_unix_ms)
        .max()
    {
        return checked >= job.started_at_unix_ms;
    }
    event
        .started_at_unix_ms
        .is_none_or(|started| started >= job.started_at_unix_ms)
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
                forget_policy_probe_job(job_id);
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
#[path = "manual_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
#[path = "ipc_tests.rs"]
mod ipc_tests;

pub(super) fn complete_policy_jobs(app_handle: &AppHandle, event: &GuiPolicyProbeCompletedEvent) {
    let state = app_handle.state::<AppState>();
    let current_scope = state.client_core_snapshot().scope;
    let (reachable, latency_ms, failed) = policy_probe_summary(event);
    let message =
        (failed > 0).then(|| format!("{failed}/{} policy members failed", event.members.len()));
    let observed_at_unix_ms = event
        .completed_at_unix_ms
        .unwrap_or_else(common::now_unix_ms);
    let matching_jobs: Vec<_> = state
        .list_client_probe_jobs(None)
        .into_iter()
        .filter(|job| {
            job.state == ProbeJobState::Running
                && job.kind == ProbeJobKind::ManualPolicy
                && job.scope == current_scope
                && job.target_tags.contains(&event.policy_tag)
                && policy_completion_matches_job(event, job)
        })
        .collect();

    for job in matching_jobs {
        let update = state.record_client_probe_result(
            job.id,
            &job.scope,
            ProbeTargetResult {
                target_tag: event.policy_tag.clone(),
                reachable,
                latency_ms,
                message: message.clone(),
                source: ProbeObservationSource::ManualPolicy,
                observed_at_unix_ms,
            },
        );
        if let Some(update) = update {
            let _ = app_handle.emit(PROBE_JOB_UPDATED_EVENT, update);
        }
        forget_policy_probe_operation(job.id, &event.policy_tag);
    }
}
