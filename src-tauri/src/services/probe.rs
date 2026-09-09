//! Passive kernel policy observations remain available in every product.
#[cfg(feature = "tool-node-probe")]
pub(crate) mod runtime;
use crate::client_core::{ProbeJobKind, ProbeObservation, ProbeObservationSource};
use crate::models::gui_core::{GuiPolicyGroup, GuiPolicyMember, GuiPolicyProbeCompletedEvent};
use crate::services::common;
use crate::state::app_state::AppState;
use tauri::{AppHandle, Emitter, Manager};

pub const CLIENT_CORE_UPDATED_EVENT: &str = "client-core:updated";
#[cfg(feature = "tool-node-probe")]
mod manual;
#[cfg(feature = "tool-node-probe")]
pub(crate) use manual::forget_policy_probe_job;
#[cfg(feature = "tool-node-probe")]
pub use manual::{
    normalize_start_request, run_probe_job, spawn_probe_timeout, PROBE_JOB_UPDATED_EVENT,
};

fn policy_member_reachable(member: &GuiPolicyMember) -> bool {
    member
        .alive
        .unwrap_or_else(|| member.delay_ms.is_some() && member.last_error.is_none())
}

/// Summarize a URLTest completion from the effective selected route rather than
/// requiring every member to succeed. A partial member failure must not turn a
/// healthy selected outbound with a real latency into a group-level timeout.
fn policy_probe_summary(event: &GuiPolicyProbeCompletedEvent) -> (bool, Option<u64>, usize) {
    let failed = event
        .members
        .iter()
        .filter(|member| !policy_member_reachable(member))
        .count();

    if let Some(selected) = event
        .selected
        .as_deref()
        .and_then(|selected| event.members.iter().find(|member| member.tag == selected))
    {
        return (policy_member_reachable(selected), selected.delay_ms, failed);
    }

    let reachable = event.members.iter().any(policy_member_reachable);
    let latency_ms = event
        .members
        .iter()
        .filter(|member| policy_member_reachable(member))
        .filter_map(|member| member.delay_ms)
        .min();
    (reachable, latency_ms, failed)
}

/// Reconcile a normalized Zero policy completion event into its exact manual
/// policy job. Automatic startup/scheduled observations never complete a
/// manual request.
pub fn record_policy_probe_completed(app_handle: &AppHandle, event: &GuiPolicyProbeCompletedEvent) {
    let scheduled =
        crate::kernel::zero::events::policy_probe_is_automatic(event.trigger.as_deref());

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
    let (reachable, latency_ms, failed) = policy_probe_summary(event);
    let message =
        (failed > 0).then(|| format!("{failed}/{} policy members failed", event.members.len()));

    for member in &event.members {
        state.record_client_probe_observation(ProbeObservation {
            scope: current_scope.clone(),
            job_kind: observation_kind,
            target_tag: member.tag.clone(),
            reachable: policy_member_reachable(member),
            latency_ms: member.delay_ms,
            message: member.last_error.clone(),
            source: observation_source,
            observed_at_unix_ms: member.last_checked_unix_ms.unwrap_or(observed_at_unix_ms),
            policy_tag: Some(event.policy_tag.clone()),
            // The concrete member is already the observation target. The
            // policy's selected route belongs only to the group summary; if it
            // is copied here a leaf node's history is mislabeled as another
            // target whenever URLTest changes its winner.
            selected_tag: None,
        });
    }

    state.record_client_probe_observation(ProbeObservation {
        scope: current_scope.clone(),
        job_kind: observation_kind,
        target_tag: event.policy_tag.clone(),
        reachable,
        latency_ms,
        message: message.clone(),
        source: observation_source,
        observed_at_unix_ms,
        policy_tag: Some(event.policy_tag.clone()),
        selected_tag: event.selected.clone(),
    });
    let _ = app_handle.emit(CLIENT_CORE_UPDATED_EVENT, state.client_core_snapshot());

    #[cfg(feature = "tool-node-probe")]
    manual::complete_policy_jobs(app_handle, event);
}

/// Recover manual policy jobs after an event gap by comparing the latest
/// kernel policy snapshot with each job's request time.
pub fn reconcile_policy_snapshot(app_handle: &AppHandle, groups: &[GuiPolicyGroup]) {
    for group in groups.iter().filter(|group| is_urltest_kind(&group.kind)) {
        let completed_at_unix_ms = group
            .outbounds
            .iter()
            .filter_map(|member| member.last_checked_unix_ms)
            .max();
        let Some(completed_at_unix_ms) = completed_at_unix_ms else {
            continue;
        };
        record_policy_probe_completed(
            app_handle,
            &GuiPolicyProbeCompletedEvent {
                operation_id: None,
                policy_tag: group.name.clone(),
                trigger: Some("scheduled".to_string()),
                url: None,
                started_at_unix_ms: None,
                completed_at_unix_ms: Some(completed_at_unix_ms),
                duration_ms: None,
                selected: group.selected.clone(),
                members: group.outbounds.clone(),
            },
        );
    }
}

fn is_urltest_kind(kind: &str) -> bool {
    matches!(
        kind.trim().to_ascii_lowercase().as_str(),
        "url_test" | "urltest" | "url-test"
    )
}
