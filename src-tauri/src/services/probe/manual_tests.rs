use super::{
    expected_policy_probe_operation, forget_policy_probe_job, forget_policy_probe_operation,
    normalize_outbound_probe_failure, policy_completion_is_fresh, policy_completion_matches_job,
    policy_probe_summary, policy_probe_timeout_ms, remember_policy_probe_operation,
};
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
        operation_id: None,
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
fn manual_policy_completion_requires_the_acknowledged_operation() {
    let job = job(1_000);
    let mut completion = event(Some(1_000), Some(1_100), Some(1_050));
    remember_policy_probe_operation(job.id, "auto", "manual-op".to_owned());

    completion.operation_id = Some("scheduled-op".to_owned());
    assert!(!policy_completion_matches_job(&completion, &job));
    completion.operation_id = Some("manual-op".to_owned());
    assert!(policy_completion_matches_job(&completion, &job));

    forget_policy_probe_operation(job.id, "auto");
}

#[test]
fn terminal_policy_job_forgets_every_target_operation() {
    let job_id = ProbeJobId(999);
    remember_policy_probe_operation(job_id, "auto-a", "manual-a".to_owned());
    remember_policy_probe_operation(job_id, "auto-b", "manual-b".to_owned());

    forget_policy_probe_job(job_id);

    assert_eq!(expected_policy_probe_operation(job_id, "auto-a"), None);
    assert_eq!(expected_policy_probe_operation(job_id, "auto-b"), None);
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
    assert!(policy_completion_is_fresh(
        &event(Some(800), Some(1_100), Some(800)),
        &job
    ));
}

#[test]
fn outbound_timeout_hides_urltest_implementation_detail() {
    let failure = normalize_outbound_probe_failure(Some("urltest probe timed out"), None);
    assert_eq!(failure.client_error_code, "probe_timeout");
    assert_eq!(failure.client_message, "节点延迟测速超时");
}

#[test]
fn unknown_outbound_failure_keeps_its_diagnostic_detail() {
    let failure = normalize_outbound_probe_failure(Some("tls handshake failed"), None);
    assert_eq!(failure.client_error_code, "probe_failed");
    assert_eq!(failure.client_message, "tls handshake failed");
}

#[test]
fn policy_timeout_scales_for_large_groups_and_keeps_a_ten_minute_cap() {
    assert_eq!(policy_probe_timeout_ms(1), 60_000);
    assert_eq!(policy_probe_timeout_ms(50), 515_000);
    assert_eq!(policy_probe_timeout_ms(100), 600_000);
}

#[test]
fn timestamp_free_legacy_completion_has_deterministic_compatibility_fallback() {
    assert!(policy_completion_is_fresh(
        &event(None, None, None),
        &job(1_000)
    ));
}

#[test]
fn partial_failure_keeps_healthy_selected_member_latency() {
    let mut completion = event(Some(1_000), Some(1_100), Some(1_050));
    completion.members[0].delay_ms = Some(80);
    completion.members.push(GuiPolicyMember {
        tag: "node-b".to_string(),
        kind: None,
        selected: false,
        alive: Some(false),
        delay_ms: None,
        last_checked_unix_ms: Some(1_050),
        last_error: Some("timeout".to_string()),
    });

    let (reachable, latency_ms, failed) = policy_probe_summary(&completion);
    assert!(reachable);
    assert_eq!(latency_ms, Some(80));
    assert_eq!(failed, 1);
}

#[test]
fn failed_selected_member_does_not_borrow_healthy_candidates_latency() {
    let mut completion = event(Some(1_000), Some(1_100), Some(1_050));
    completion.members[0].alive = Some(false);
    completion.members[0].delay_ms = None;
    completion.members[0].last_error = Some("timeout".to_string());
    completion.members.push(GuiPolicyMember {
        tag: "node-b".to_string(),
        kind: None,
        selected: false,
        alive: Some(true),
        delay_ms: Some(40),
        last_checked_unix_ms: Some(1_050),
        last_error: None,
    });

    let (reachable, latency_ms, failed) = policy_probe_summary(&completion);
    assert!(!reachable);
    assert_eq!(latency_ms, None);
    assert_eq!(failed, 1);
}

#[test]
fn legacy_completion_without_selected_uses_best_healthy_latency() {
    let mut completion = event(Some(1_000), Some(1_100), Some(1_050));
    completion.selected = None;
    completion.members[0].delay_ms = Some(80);
    completion.members.push(GuiPolicyMember {
        tag: "node-b".to_string(),
        kind: None,
        selected: false,
        alive: Some(true),
        delay_ms: Some(40),
        last_checked_unix_ms: Some(1_050),
        last_error: None,
    });

    let (reachable, latency_ms, failed) = policy_probe_summary(&completion);
    assert!(reachable);
    assert_eq!(latency_ms, Some(40));
    assert_eq!(failed, 0);
}
