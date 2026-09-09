mod support;
use support::TestHost as ClientCore;
use znet_client_core::{
    ConfigRevision, CoreInstanceId, ProbeJobKind, ProbeJobState, ProbeObservation,
    ProbeObservationSource, ProbeTargetResult, ProfileId, SnapshotRevision, StartProbeRequest,
};

fn core() -> ClientCore {
    ClientCore::new(Some(ProfileId("profile-a".to_string())), ConfigRevision(10))
}

fn request(tags: &[&str]) -> StartProbeRequest {
    StartProbeRequest {
        kind: ProbeJobKind::Outbound,
        target_tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
        timeout_ms: Some(10_000),
    }
}

fn result(tag: &str, reachable: bool, at: u64) -> ProbeTargetResult {
    ProbeTargetResult {
        target_tag: tag.to_string(),
        reachable,
        latency_ms: reachable.then_some(42),
        message: None,
        source: ProbeObservationSource::ManualOutbound,
        observed_at_unix_ms: at,
    }
}

#[test]
fn configuration_commit_advances_scope_and_snapshot_atomically() {
    let mut core = core();

    core.configuration_committed(
        Some(ProfileId("profile-b".to_string())),
        ConfigRevision(20),
        10,
    );
    let snapshot = core.snapshot();

    assert_eq!(snapshot.revision, SnapshotRevision(2));
    assert_eq!(
        snapshot.scope.profile_id,
        Some(ProfileId("profile-b".to_string()))
    );
    assert_eq!(snapshot.scope.config_revision, ConfigRevision(20));
    assert_eq!(snapshot.scope.core_instance_id, CoreInstanceId(0));
}

#[test]
fn core_restart_changes_instance_without_changing_config_revision() {
    let mut core = core();

    core.core_instance_started(10);
    core.core_instance_started(20);
    let snapshot = core.snapshot();

    assert_eq!(snapshot.revision, SnapshotRevision(3));
    assert_eq!(snapshot.scope.config_revision, ConfigRevision(10));
    assert_eq!(snapshot.scope.core_instance_id, CoreInstanceId(2));
}

#[test]
fn identical_tags_in_different_profiles_have_distinct_ids() {
    let first = core().snapshot().scope.node_id("shared").unwrap();
    let second = ClientCore::new(Some(ProfileId("profile-b".to_string())), ConfigRevision(10))
        .snapshot()
        .scope
        .node_id("shared")
        .unwrap();

    assert_ne!(first, second);
}

#[test]
fn repeated_active_request_is_deduplicated() {
    let mut core = core();
    let first = core.start_probe(request(&["a", "b", "a"]), 100).unwrap();
    let second = core.start_probe(request(&["a", "b"]), 101).unwrap();

    assert!(first.created);
    assert!(!second.created);
    assert_eq!(first.job.id, second.job.id);
    assert_eq!(first.job.target_tags, vec!["a", "b"]);
}

#[test]
fn duplicate_and_stale_results_are_ignored() {
    let mut core = core();
    let started = core.start_probe(request(&["a"]), 100).unwrap().job;
    let stale_scope = {
        let mut scope = started.scope.clone();
        scope.config_revision.0 += 1;
        scope
    };

    assert!(core
        .record_probe_result(started.id, &stale_scope, result("a", true, 101))
        .is_none());
    assert!(core
        .record_probe_result(started.id, &started.scope, result("a", true, 102))
        .is_some());
    assert!(core
        .record_probe_result(started.id, &started.scope, result("a", false, 103))
        .is_none());
    assert_eq!(
        core.get_probe_job(started.id).unwrap().state,
        ProbeJobState::Completed
    );
}

#[test]
fn mixed_results_finish_as_partially_failed() {
    let mut core = core();
    let job = core.start_probe(request(&["a", "b"]), 100).unwrap().job;

    core.record_probe_result(job.id, &job.scope, result("a", true, 101));
    core.record_probe_result(job.id, &job.scope, result("b", false, 102));

    let completed = core.get_probe_job(job.id).unwrap();
    assert_eq!(completed.state, ProbeJobState::PartiallyFailed);
    assert_eq!((completed.succeeded, completed.failed), (1, 1));
}

#[test]
fn config_change_and_restart_use_distinct_invalidation_states() {
    let mut core = core();
    let config_job = core.start_probe(request(&["a"]), 100).unwrap().job;
    core.configuration_committed(
        Some(ProfileId("profile-b".to_string())),
        ConfigRevision(20),
        101,
    );
    assert_eq!(
        core.get_probe_job(config_job.id).unwrap().state,
        ProbeJobState::InvalidatedByConfigChange
    );

    let restart_job = core.start_probe(request(&["a"]), 102).unwrap().job;
    core.core_instance_started(103);
    assert_eq!(
        core.get_probe_job(restart_job.id).unwrap().state,
        ProbeJobState::InvalidatedByCoreRestart
    );
}

#[test]
fn lost_core_invalidates_immediately_before_a_replacement_starts() {
    let mut core = core();
    core.core_instance_started(50);
    let generation = core.snapshot().scope.core_instance_id;
    let job = core.start_probe(request(&["a"]), 100).unwrap().job;

    core.core_instance_lost(101);
    let offline = core.snapshot();
    assert_eq!(
        offline.source_status,
        znet_client_core::SourceStatus::Offline
    );
    assert_eq!(offline.scope.core_instance_id, generation);
    assert_eq!(
        core.get_probe_job(job.id).unwrap().state,
        ProbeJobState::InvalidatedByCoreRestart
    );

    core.core_instance_started(102);
    assert!(core.snapshot().scope.core_instance_id > generation);
}

#[test]
fn one_hundred_targets_complete_once_even_when_results_are_reordered() {
    let mut core = core();
    let tags: Vec<_> = (0..100).map(|index| format!("node-{index:03}")).collect();
    let job = core
        .start_probe(
            StartProbeRequest {
                kind: ProbeJobKind::Outbound,
                target_tags: tags.clone(),
                timeout_ms: Some(30_000),
            },
            100,
        )
        .unwrap()
        .job;

    for (index, tag) in tags.iter().rev().enumerate() {
        assert!(core
            .record_probe_result(job.id, &job.scope, result(tag, true, 101 + index as u64))
            .is_some());
    }
    assert!(core
        .record_probe_result(job.id, &job.scope, result(&tags[0], false, 500))
        .is_none());

    let completed = core.get_probe_job(job.id).unwrap();
    assert_eq!(completed.state, ProbeJobState::Completed);
    assert_eq!(
        (completed.completed, completed.succeeded, completed.failed),
        (100, 100, 0)
    );
    assert!(core.snapshot().active_probe_jobs.is_empty());
}

#[test]
fn snapshot_recovers_running_job_after_page_recreation_or_stream_gap() {
    let mut core = core();
    let job = core.start_probe(request(&["a", "b"]), 100).unwrap().job;
    core.record_probe_result(job.id, &job.scope, result("b", true, 101));

    // A newly-created consumer has no local session state and can recover
    // the complete in-flight truth from this snapshot alone.
    let recovered = core.snapshot().active_probe_jobs;
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].id, job.id);
    assert_eq!(recovered[0].completed, 1);
    assert_eq!(recovered[0].target_tags, vec!["a", "b"]);
}

#[test]
fn timeout_marks_only_missing_targets_and_late_results_are_ignored() {
    let mut core = core();
    let job = core.start_probe(request(&["a", "b"]), 100).unwrap().job;
    core.record_probe_result(job.id, &job.scope, result("a", true, 101));

    let timed_out = core.timeout_probe(job.id, 10_100).unwrap();
    assert_eq!(timed_out.state, ProbeJobState::TimedOut);
    assert_eq!(
        (timed_out.completed, timed_out.succeeded, timed_out.failed),
        (1, 1, 0)
    );
    assert!(core
        .record_probe_result(job.id, &job.scope, result("b", true, 10_101))
        .is_none());

    let observations = core.observations_for_config(&job.scope);
    let missing = observations
        .iter()
        .find(|observation| observation.target_tag == "b")
        .unwrap();
    assert!(!missing.reachable);
    assert_eq!(missing.message.as_deref(), Some("probe job timed out"));
}

#[test]
fn cancellation_is_terminal_and_drops_in_flight_results() {
    let mut core = core();
    let job = core.start_probe(request(&["a"]), 100).unwrap().job;

    assert_eq!(
        core.cancel_probe(job.id, 101).unwrap().state,
        ProbeJobState::Cancelled
    );
    assert!(core
        .record_probe_result(job.id, &job.scope, result("a", true, 102))
        .is_none());
    assert!(core.snapshot().active_probe_jobs.is_empty());
}

#[test]
fn all_failed_results_use_failed_terminal_state() {
    let mut core = core();
    let job = core.start_probe(request(&["a", "b"]), 100).unwrap().job;

    core.record_probe_result(job.id, &job.scope, result("a", false, 101));
    core.record_probe_result(job.id, &job.scope, result("b", false, 102));

    assert_eq!(
        core.get_probe_job(job.id).unwrap().state,
        ProbeJobState::Failed
    );
}

#[test]
fn scheduled_observation_does_not_complete_overlapping_manual_job() {
    let mut core = core();
    let job = core
        .start_probe(
            StartProbeRequest {
                kind: ProbeJobKind::ManualPolicy,
                target_tags: vec!["auto".to_string()],
                timeout_ms: None,
            },
            100,
        )
        .unwrap()
        .job;
    assert!(core.record_observation(ProbeObservation {
        scope: job.scope.clone(),
        job_kind: ProbeJobKind::ScheduledPolicyObservation,
        target_tag: "node-a".to_string(),
        reachable: true,
        latency_ms: Some(20),
        message: None,
        source: ProbeObservationSource::ScheduledPolicy,
        observed_at_unix_ms: 101,
        policy_tag: Some("auto".to_string()),
        selected_tag: Some("node-a".to_string()),
    }));

    assert_eq!(
        core.get_probe_job(job.id).unwrap().state,
        ProbeJobState::Running
    );
    assert_eq!(
        core.observations_for_config(&job.scope)[0].source,
        ProbeObservationSource::ScheduledPolicy
    );
}

#[test]
fn history_is_config_scoped_but_survives_core_generation_changes() {
    let mut core = core();
    let original_scope = core.snapshot().scope;
    assert!(core.record_observation(ProbeObservation {
        scope: original_scope.clone(),
        job_kind: ProbeJobKind::Outbound,
        target_tag: "shared".to_string(),
        reachable: true,
        latency_ms: Some(25),
        message: None,
        source: ProbeObservationSource::ManualOutbound,
        observed_at_unix_ms: 100,
        policy_tag: None,
        selected_tag: None,
    }));

    core.core_instance_started(101);
    let restarted_scope = core.snapshot().scope;
    assert_ne!(
        original_scope.core_instance_id,
        restarted_scope.core_instance_id
    );
    assert_eq!(core.observations_for_config(&restarted_scope).len(), 1);

    core.configuration_committed(
        Some(ProfileId("profile-a".to_string())),
        ConfigRevision(11),
        102,
    );
    assert!(core
        .observations_for_config(&core.snapshot().scope)
        .is_empty());
}

#[test]
fn same_tag_in_two_revisions_has_distinct_identity() {
    let first = core().snapshot().scope.node_id("shared").unwrap();
    let second = ClientCore::new(Some(ProfileId("profile-a".to_string())), ConfigRevision(11))
        .snapshot()
        .scope
        .node_id("shared")
        .unwrap();

    assert_ne!(first, second);
}

#[test]
fn observation_sources_cannot_masquerade_as_another_job_kind() {
    let mut core = core();
    let job = core.start_probe(request(&["a"]), 100).unwrap().job;
    let mut wrong_result = result("a", true, 101);
    wrong_result.source = ProbeObservationSource::ScheduledPolicy;
    assert!(core
        .record_probe_result(job.id, &job.scope, wrong_result)
        .is_none());

    assert!(!core.record_observation(ProbeObservation {
        scope: job.scope.clone(),
        job_kind: ProbeJobKind::ManualPolicy,
        target_tag: "a".to_string(),
        reachable: true,
        latency_ms: Some(10),
        message: None,
        source: ProbeObservationSource::ScheduledPolicy,
        observed_at_unix_ms: 102,
        policy_tag: None,
        selected_tag: None,
    }));
}
