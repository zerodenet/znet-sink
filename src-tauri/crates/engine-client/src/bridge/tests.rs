use super::*;
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Duration,
};
use znet_client_core::capability::{Budget, Manager, Policy};

fn binding() -> RuntimeBinding {
    RuntimeBinding {
        runtime: RuntimeIdentity {
            core_instance_id: "actual-peer-a".into(),
            config_revision: 1,
        },
        profile_id: "profile-a".into(),
        client_generation: 1,
    }
}
fn caller() -> HostCaller {
    HostCaller::from_window_label("main").unwrap()
}
fn auth() -> AuthContext {
    AuthContext {
        subject: Some("trusted-host".into()),
        permissions: vec![zero_api::Permission::Admin],
    }
}
fn grant(binding: &RuntimeBinding, capability: &str) -> (Policy, Lease) {
    let manager = Manager::default();
    let permission = Permission::new(capability, binding.permission_scope());
    let grants = BTreeSet::from([permission]);
    let policy = manager
        .admit(
            HOST_IDENTITY.into(),
            grants.clone(),
            grants.clone(),
            &grants,
        )
        .unwrap();
    policy.authorize(grants, Duration::from_secs(10)).unwrap();
    let lease = policy
        .begin(
            Budget {
                calls: 1,
                resource_bytes: MAX_RESULT_BYTES,
                timeout: Duration::from_millis(150),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
    (policy, lease)
}
struct Backend {
    calls: AtomicUsize,
    command_calls: AtomicUsize,
    runtime: Mutex<Value>,
    on_payload: Option<Box<dyn Fn() + Send + Sync>>,
    delay: Duration,
    accepted: Option<bool>,
    refused: bool,
    wrong_target: bool,
}
impl Default for Backend {
    fn default() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            command_calls: AtomicUsize::new(0),
            runtime: Mutex::new(json!({"core_instance_id":"actual-peer-a","config_revision":"1"})),
            on_payload: None,
            delay: Duration::ZERO,
            accepted: Some(true),
            refused: false,
            wrong_target: false,
        }
    }
}
impl QueryTransport for Backend {
    type Error = &'static str;
    async fn query(&self, request: Value) -> Result<Value, Self::Error> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if request.get("runtime").is_some() {
            return Ok(self.runtime.lock().unwrap().clone());
        }
        if let Some(hook) = &self.on_payload {
            hook();
        }
        tokio::time::sleep(self.delay).await;
        Ok(json!({"items":[],"token":"not-in-receipt"}))
    }
}
impl BridgeTransport for Backend {
    async fn command(&self, request: CommandRequest) -> Result<Value, Self::Error> {
        self.command_calls.fetch_add(1, Ordering::Relaxed);
        if self.refused {
            return Err("permission_denied");
        }
        if let Some(hook) = &self.on_payload {
            hook();
        }
        tokio::time::sleep(self.delay).await;
        let result = match request {
            CommandRequest::FlowClose(command) => {
                json!({"flow_id":if self.wrong_target{"another-flow"}else{&command.flow_id},"closed":true})
            }
            CommandRequest::PacketRouteClose(command) => {
                json!({"closed":true,"route":{"route_id":command.route_id,"core_instance_id":command.expected_core_instance_id,"config_revision":command.expected_config_revision}})
            }
            _ => json!({}),
        };
        Ok(self.accepted.map_or(
            json!({}),
            |accepted| json!({"accepted":accepted,"result":result}),
        ))
    }
    fn failure_is_rejection(&self, error: &Self::Error) -> bool {
        *error == "permission_denied"
    }
}
fn query() -> BridgeRequest {
    BridgeRequest::Query(QueryRequest::ActiveFlows(zero_api::FlowListQuery {
        limit: Some(20),
        filter: Default::default(),
    }))
}
fn command() -> BridgeRequest {
    BridgeRequest::Command(CommandRequest::FlowClose(zero_api::FlowCloseCommand {
        flow_id: "fixed-flow-id".into(),
    }))
}
fn ready() -> RuntimeBridge {
    let bridge = RuntimeBridge::default();
    bridge.bind(binding()).unwrap();
    bridge
}

#[test]
fn unavailable_and_guest_caller_cannot_issue_runtime_requests() {
    assert!(matches!(
        RuntimeBridge::default().issue(&caller()),
        Err(IssueError::Unavailable)
    ));
    for label in ["guest.publisher.component", "traffic-ball", "", "MAIN"] {
        assert!(matches!(
            HostCaller::from_window_label(label),
            Err(IssueError::Forbidden)
        ));
    }
    let bridge = ready();
    let first = bridge.issue(&caller()).unwrap();
    let second = bridge.issue(&caller()).unwrap();
    assert_ne!(first.operation_id(), second.operation_id());
    assert_eq!(first.binding(), &binding());
}
#[tokio::test]
async fn genuine_pinned_query_is_checked_before_and_after_and_receipt_has_no_payload() {
    let bridge = ready();
    let backend = Backend::default();
    let (_, lease) = grant(&binding(), "runtime.query");
    let ticket = bridge.issue(&caller()).unwrap();
    let result = bridge
        .call(&backend, ticket, query(), &lease, &auth())
        .await
        .unwrap();
    assert_eq!(backend.calls.load(Ordering::Relaxed), 3);
    assert_eq!(result.receipt.state, ReceiptState::Confirmed);
    assert!(!serde_json::to_string(&result.receipt)
        .unwrap()
        .contains("not-in-receipt"));
    assert_eq!(result.receipt.config_revision, "1");
}
#[tokio::test]
async fn native_permission_and_host_lease_intersection_is_checked_before_io() {
    let bridge = ready();
    let backend = Backend::default();
    let (_, lease) = grant(&binding(), "runtime.control");
    let read_only = AuthContext {
        subject: None,
        permissions: vec![zero_api::Permission::Read],
    };
    assert!(matches!(
        bridge
            .call(
                &backend,
                bridge.issue(&caller()).unwrap(),
                command(),
                &lease,
                &read_only
            )
            .await,
        Err(BridgeError::Forbidden)
    ));
    assert_eq!(backend.calls.load(Ordering::Relaxed), 0);
    let (_, lease) = grant(&binding(), "runtime.query");
    assert!(matches!(
        bridge
            .call(
                &backend,
                bridge.issue(&caller()).unwrap(),
                command(),
                &lease,
                &auth()
            )
            .await,
        Err(BridgeError::Capability(CapabilityError::PermissionDenied))
    ));
    assert_eq!(backend.command_calls.load(Ordering::Relaxed), 0);
}
#[tokio::test]
async fn rebinding_or_kernel_revision_change_blocks_late_result_delivery() {
    let bridge = ready();
    let target = bridge.clone();
    let backend = Backend {
        on_payload: Some(Box::new(move || {
            let mut updated = binding();
            updated.client_generation = 2;
            target.bind(updated).unwrap();
        })),
        ..Default::default()
    };
    let (_, lease) = grant(&binding(), "runtime.query");
    assert!(matches!(
        bridge
            .call(
                &backend,
                bridge.issue(&caller()).unwrap(),
                query(),
                &lease,
                &auth()
            )
            .await,
        Err(BridgeError::StaleBinding)
    ));
    let bridge = ready();
    let backend = Backend::default();
    *backend.runtime.lock().unwrap() =
        json!({"core_instance_id":"actual-peer-a","config_revision":2});
    let (_, lease) = grant(&binding(), "runtime.control");
    assert!(matches!(
        bridge
            .call(
                &backend,
                bridge.issue(&caller()).unwrap(),
                command(),
                &lease,
                &auth()
            )
            .await,
        Err(BridgeError::StaleBinding)
    ));
    assert_eq!(backend.command_calls.load(Ordering::Relaxed), 0);
}
#[tokio::test]
async fn late_revocation_denies_delivery_and_timeout_does_not_retry_command() {
    let bridge = ready();
    let (policy, lease) = grant(&binding(), "runtime.control");
    let backend = Backend {
        on_payload: Some(Box::new(move || policy.revoke())),
        ..Default::default()
    };
    let ticket = bridge.issue(&caller()).unwrap();
    let id = ticket.operation_id().to_owned();
    assert!(matches!(
        bridge
            .call(&backend, ticket, command(), &lease, &auth())
            .await,
        Err(BridgeError::Capability(CapabilityError::Revoked))
    ));
    assert_eq!(backend.command_calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        bridge.receipt(&caller(), &id).unwrap().state,
        ReceiptState::DeliveryDenied
    );
    let bridge = ready();
    let (_, lease) = grant(&binding(), "runtime.control");
    let backend = Backend {
        delay: Duration::from_millis(200),
        ..Default::default()
    };
    let ticket = bridge.issue(&caller()).unwrap();
    let id = ticket.operation_id().to_owned();
    assert!(matches!(
        bridge
            .call(&backend, ticket, command(), &lease, &auth())
            .await,
        Err(BridgeError::Unconfirmed | BridgeError::Capability(CapabilityError::Deadline))
    ));
    assert_eq!(backend.command_calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        bridge.receipt(&caller(), &id).unwrap().state,
        ReceiptState::Unknown
    );
}
#[tokio::test]
async fn missing_ack_is_unknown_explicit_refusal_is_rejected_and_no_arbitrary_commands() {
    let bridge = ready();
    let backend = Backend {
        accepted: None,
        ..Default::default()
    };
    let (_, lease) = grant(&binding(), "runtime.control");
    let ticket = bridge.issue(&caller()).unwrap();
    let id = ticket.operation_id().to_owned();
    assert!(matches!(
        bridge
            .call(&backend, ticket, command(), &lease, &auth())
            .await,
        Err(BridgeError::Unconfirmed)
    ));
    assert_eq!(
        bridge.receipt(&caller(), &id).unwrap().state,
        ReceiptState::Unknown
    );
    let bridge = ready();
    let backend = Backend {
        accepted: Some(false),
        ..Default::default()
    };
    let (_, lease) = grant(&binding(), "runtime.control");
    assert_eq!(
        bridge
            .call(
                &backend,
                bridge.issue(&caller()).unwrap(),
                command(),
                &lease,
                &auth()
            )
            .await
            .unwrap()
            .receipt
            .state,
        ReceiptState::Rejected
    );
    let (_, lease) = grant(&binding(), "runtime.query");
    assert!(matches!(
        bridge
            .call(
                &backend,
                bridge.issue(&caller()).unwrap(),
                BridgeRequest::Query(QueryRequest::Config(zero_api::ConfigQuery)),
                &lease,
                &auth()
            )
            .await,
        Err(BridgeError::Unsupported)
    ));
}
#[test]
fn receipt_budget_never_evicts_inflight_tickets_and_drop_records_without_effects() {
    let bridge = ready();
    let mut tickets = (0..MAX_RECEIPTS)
        .map(|_| bridge.issue(&caller()).unwrap())
        .collect::<Vec<_>>();
    assert!(matches!(bridge.issue(&caller()), Err(IssueError::Busy)));
    let discarded = tickets.pop().unwrap();
    let id = discarded.operation_id().to_owned();
    drop(discarded);
    assert_eq!(
        bridge.receipt(&caller(), &id).unwrap().state,
        ReceiptState::NotSubmitted
    );
    assert!(bridge.issue(&caller()).is_ok());
}

#[tokio::test]
async fn invalidation_rebinding_same_identity_never_resurrects_ticket_or_late_reply() {
    let bridge = ready();
    let ticket = bridge.issue(&caller()).unwrap();
    bridge.invalidate();
    bridge.bind(binding()).unwrap();
    let backend = Backend::default();
    let (_, lease) = grant(&binding(), "runtime.query");
    assert!(matches!(
        bridge
            .call(&backend, ticket, query(), &lease, &auth())
            .await,
        Err(BridgeError::StaleBinding)
    ));
    assert_eq!(backend.calls.load(Ordering::Relaxed), 0);
    let bridge = ready();
    let target = bridge.clone();
    let backend = Backend {
        on_payload: Some(Box::new(move || {
            target.invalidate();
            target.bind(binding()).unwrap();
        })),
        ..Default::default()
    };
    let (_, lease) = grant(&binding(), "runtime.query");
    assert!(matches!(
        bridge
            .call(
                &backend,
                bridge.issue(&caller()).unwrap(),
                query(),
                &lease,
                &auth()
            )
            .await,
        Err(BridgeError::StaleBinding)
    ));
}
#[tokio::test]
async fn close_ack_is_bound_to_actual_target_and_explicit_transport_refusal_is_not_unknown() {
    let bridge = ready();
    let backend = Backend::default();
    let (_, lease) = grant(&binding(), "runtime.control");
    assert_eq!(
        bridge
            .call(
                &backend,
                bridge.issue(&caller()).unwrap(),
                command(),
                &lease,
                &auth()
            )
            .await
            .unwrap()
            .receipt
            .state,
        ReceiptState::Confirmed
    );
    let (_, lease) = grant(&binding(), "runtime.control");
    let wrong = Backend {
        wrong_target: true,
        ..Default::default()
    };
    assert!(matches!(
        bridge
            .call(
                &wrong,
                bridge.issue(&caller()).unwrap(),
                command(),
                &lease,
                &auth()
            )
            .await,
        Err(BridgeError::Unconfirmed)
    ));
    let (_, lease) = grant(&binding(), "runtime.control");
    let refused = Backend {
        refused: true,
        ..Default::default()
    };
    let ticket = bridge.issue(&caller()).unwrap();
    let id = ticket.operation_id().to_owned();
    assert!(matches!(
        bridge
            .call(&refused, ticket, command(), &lease, &auth())
            .await,
        Err(BridgeError::Rejected("permission_denied"))
    ));
    assert_eq!(
        bridge.receipt(&caller(), &id).unwrap().state,
        ReceiptState::Rejected
    );
}
fn event(sequence: u64) -> zero_api::RawApiEvent {
    let mut event = zero_api::ApiEvent::new(
        sequence.to_string(),
        zero_api::event_type::FLOW_STARTED,
        sequence,
        json!({"config":{"password":"secret"}}),
    );
    event.core_instance_id = Some("actual-peer-a".into());
    event.config_revision = Some(1);
    event.sequence = Some(sequence);
    event
}
#[test]
fn listener_cancel_has_no_runtime_effect_and_foreground_epoch_invalidates_old_listener() {
    let bridge = ready();
    let buffer = events::EventBuffer::new(bridge.clone());
    buffer.rebind(binding(), 0).unwrap();
    let old = buffer.listen(&caller()).unwrap();
    let active = buffer.listen(&caller()).unwrap();
    old.cancel();
    buffer.publish(&event(1));
    assert!(matches!(
        buffer.replay(&old, 0, 64),
        Err(events::ListenerError::Cancelled)
    ));
    assert_eq!(buffer.replay(&active, 0, 64).unwrap().events.len(), 1);
    assert_eq!(bridge.binding(), Some(binding()));
    buffer.rebind(binding(), 1).unwrap();
    assert!(matches!(
        buffer.replay(&active, 1, 64),
        Err(events::ListenerError::StaleBinding)
    ));
    let new = buffer.listen(&caller()).unwrap();
    assert!(!buffer.replay(&new, 1, 64).unwrap().has_gap);
}
#[test]
fn bounded_replay_gap_missing_identity_and_upstream_loss_require_fresh_snapshot() {
    let bridge = ready();
    let buffer = events::EventBuffer::new(bridge);
    buffer.rebind(binding(), 0).unwrap();
    let listener = buffer.listen(&caller()).unwrap();
    for sequence in 1..=130 {
        buffer.publish(&event(sequence));
    }
    let replay = buffer.replay(&listener, 0, 64).unwrap();
    assert!(replay.has_gap);
    assert!(replay.events.is_empty());
    let replay = buffer.replay(&listener, 2, 64).unwrap();
    assert!(!replay.has_gap);
    assert_eq!(replay.events.len(), 64);
    assert!(!serde_json::to_string(&replay).unwrap().contains("secret"));
    buffer.mark_gap();
    assert!(buffer.replay(&listener, 130, 64).unwrap().has_gap);
    buffer.rebind(binding(), 130).unwrap();
    let listener = buffer.listen(&caller()).unwrap();
    let mut invalid = event(131);
    invalid.core_instance_id = None;
    buffer.publish(&invalid);
    assert!(buffer.replay(&listener, 130, 64).unwrap().has_gap);
}
