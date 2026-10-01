use super::*;
use std::sync::Mutex;

struct Fake {
    row: Mutex<Value>,
    calls: Mutex<Vec<Value>>,
    failures: Vec<usize>,
    changed_on_failure: bool,
    lost_ack: bool,
    pending_ack: bool,
}
impl Fake {
    fn new(failures: Vec<usize>) -> Self {
        let mut row = super::super::tests::current();
        row["supported"]["directions"]["inbound"] = json!(true);
        row["allowed"] = json!({"inbound":true,"outbound":true});
        row["effective"] = row["allowed"].clone();
        Self {
            row: Mutex::new(row),
            calls: Mutex::new(Vec::new()),
            failures,
            changed_on_failure: false,
            lost_ack: false,
            pending_ack: false,
        }
    }
    fn snapshot(&self) -> Value {
        self.row.lock().unwrap().clone()
    }
}
impl Backend for Fake {
    async fn execute(&self, input: ControlInput, revision: u64) -> AppResult<Value> {
        let mut row = self.row.lock().unwrap();
        assert_eq!(input.endpoint_id, row["endpoint_id"]);
        assert_eq!(input.core_instance_id, row["core_instance_id"]);
        assert_eq!(
            input.expected_intent_revision,
            row["intent_revision"].as_u64().unwrap()
        );
        assert_eq!(revision, row["config_revision"].as_u64().unwrap());
        let mut calls = self.calls.lock().unwrap();
        let action = match input.action {
            Action::SetState {
                enabled,
                persistence: Persistence::RuntimeOnly,
            } => json!({"operation":"set_state","enabled":enabled}),
            Action::SetDirections {
                directions,
                persistence: Persistence::RuntimeOnly,
            } => {
                assert_eq!(row["state"], "stopped");
                json!({"operation":"set_directions","directions":directions})
            }
            _ => panic!("unexpected operation"),
        };
        calls.push(action.clone());
        let failure = self.failures.contains(&calls.len());
        if failure && self.changed_on_failure {
            row["core_instance_id"] = json!("another-core");
            return Err(AppError::core_response(
                json!({"ok":false,"error":{"code":"conflict","message":"core replaced"}}),
            ));
        }
        if failure && self.pending_ack {
            return Err(AppError {
                code: "connection_closed",
                message: "pending kernel transaction".into(),
                details: None,
            });
        }
        if failure && !self.lost_ack {
            return Err(AppError::core_response(
                json!({"ok":false,"error":{"code":"internal","message":"reconciliation failed"}}),
            ));
        }
        match action["operation"].as_str().unwrap() {
            "set_state" => {
                row["enabled"] = action["enabled"].clone();
                row["state"] = if row["enabled"] == true {
                    json!("running")
                } else {
                    json!("stopped")
                };
            }
            "set_directions" => row["allowed"] = action["directions"].clone(),
            _ => unreachable!(),
        }
        row["intent_revision"] = json!(row["intent_revision"].as_u64().unwrap() + 1);
        row["effective"] = if row["enabled"] == true {
            row["allowed"].clone()
        } else {
            json!({"inbound":false,"outbound":false})
        };
        if failure {
            Err(AppError::internal("confirmation lost"))
        } else {
            Ok(row.clone())
        }
    }
    async fn observe(&self, _id: &str) -> AppResult<Value> {
        Ok(self.snapshot())
    }
}
fn target() -> Directions {
    Directions {
        inbound: true,
        outbound: false,
    }
}

#[tokio::test]
async fn endpoint_direction_switch_stops_changes_restores_enable_with_fresh_revisions() {
    let backend = Fake::new(vec![]);
    let row = transition(&backend, backend.snapshot(), target(), 1)
        .await
        .unwrap();
    assert_eq!(row["enabled"], true);
    assert_eq!(row["allowed"], json!(target()));
    assert_eq!(row["intent_revision"], 10);
    assert_eq!(
        *backend.calls.lock().unwrap(),
        vec![
            json!({"operation":"set_state","enabled":false}),
            json!({"operation":"set_directions","directions":target()}),
            json!({"operation":"set_state","enabled":true})
        ]
    );
}
#[tokio::test]
async fn endpoint_direction_switch_recovers_each_failed_phase_without_saving_intermediate_intent() {
    for phase in [1, 2, 3] {
        let backend = Fake::new(vec![phase]);
        let original = backend.snapshot();
        let error = transition(&backend, original.clone(), target(), 1)
            .await
            .unwrap_err();
        assert!(error.message.contains("已恢复原状态"));
        let row = backend.snapshot();
        assert_eq!(row["enabled"], original["enabled"]);
        assert_eq!(row["allowed"], original["allowed"]);
    }
}
#[tokio::test]
async fn endpoint_direction_switch_does_not_adopt_unknown_revisions_or_replay_lost_confirmation() {
    for phase in [1, 2, 3] {
        let mut backend = Fake::new(vec![phase]);
        backend.lost_ack = true;
        let error = transition(&backend, backend.snapshot(), target(), 1)
            .await
            .unwrap_err();
        assert!(!error.message.contains("已恢复原状态"));
        assert_eq!(backend.calls.lock().unwrap().len(), phase);
        assert_eq!(error.details.unwrap()["recovery"]["restored"], false);
    }
}
#[tokio::test]
async fn endpoint_direction_switch_stops_recovery_on_core_change_or_compensation_failure() {
    let mut backend = Fake::new(vec![2]);
    backend.changed_on_failure = true;
    let error = transition(&backend, backend.snapshot(), target(), 1)
        .await
        .unwrap_err();
    assert!(!error.message.contains("已恢复原状态"));
    assert_eq!(backend.calls.lock().unwrap().len(), 2);
    let backend = Fake::new(vec![3, 4]);
    let error = transition(&backend, backend.snapshot(), target(), 1)
        .await
        .unwrap_err();
    assert_eq!(error.details.unwrap()["recovery"]["restored"], false);
    assert_eq!(backend.calls.lock().unwrap().len(), 4);
    assert_eq!(backend.snapshot()["enabled"], false);
}
#[test]
fn endpoint_direction_restart_is_only_needed_for_declared_live_contraction() {
    let row = Fake::new(vec![]).snapshot();
    let mut caps = super::super::tests::caps();
    caps.global_limitations =
        vec!["endpoint_live_outbound_direction_contraction_requires_stop".into()];
    assert!(requires_stop(&row, &target(), &caps));
    assert!(!requires_stop(
        &row,
        &Directions {
            inbound: false,
            outbound: true
        },
        &caps
    ));
    let mut stopped = row.clone();
    stopped["state"] = json!("stopped");
    assert!(!requires_stop(&stopped, &target(), &caps));
    caps.features
        .push("network_endpoint_operation_capabilities_v1".into());
    let mut future = row;
    future["supported"]["operation_capabilities"]["set_directions"]["live_direction_contraction"] =
        json!({"inbound":true,"outbound":true});
    assert!(!requires_stop(&future, &target(), &caps));
}

#[tokio::test]
async fn endpoint_direction_switch_does_not_claim_rollback_from_old_observation_after_disconnect() {
    for phase in [1, 2, 3] {
        let mut backend = Fake::new(vec![phase]);
        backend.pending_ack = true;
        let error = transition(&backend, backend.snapshot(), target(), 1)
            .await
            .unwrap_err();
        assert!(!error.message.contains("已恢复原状态"));
        assert_eq!(backend.calls.lock().unwrap().len(), phase);
        assert_eq!(error.details.unwrap()["recovery"]["restored"], false);
    }
}
