//! Host-owned, fixed-runtime control boundary. This is a transport adapter,
//! not a Proxy lifecycle, job queue, or native runtime registration mechanism.
use crate::{configuration::RuntimeIdentity, QueryTransport};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    future::Future,
    sync::{Arc, Mutex},
};
use zero_api::{AuthContext, CommandRequest, QueryRequest};
use znet_client_core::capability::{Error as CapabilityError, Lease, Permission};

pub const MAX_RESULT_BYTES: usize = 4 * 1024 * 1024;
const MAX_RECEIPTS: usize = 256;
pub const HOST_IDENTITY: &str = "builtin.runtime-bridge";

/// Implement on the host's pinned IPC or real Proxy-owned adapter. It must
/// never discover a replacement connection midway through this operation.
pub trait BridgeTransport: QueryTransport {
    fn command(
        &self,
        request: CommandRequest,
    ) -> impl Future<Output = Result<Value, Self::Error>> + Send;
    /// Only explicit precondition/permission/validation refusals qualify.
    /// Timeout, disconnection and internal errors may follow an actual write.
    fn failure_is_rejection(&self, _error: &Self::Error) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeBinding {
    pub runtime: RuntimeIdentity,
    pub profile_id: String,
    pub client_generation: u64,
}
impl RuntimeBinding {
    pub fn permission_scope(&self) -> String {
        // Unambiguous even when a profile/resource identity contains separators.
        serde_json::to_string(&(
            &self.runtime.core_instance_id,
            &self.profile_id,
            self.runtime.config_revision.to_string(),
            self.client_generation.to_string(),
        ))
        .expect("string tuple serializes")
    }
    fn valid(&self) -> bool {
        !self.runtime.core_instance_id.trim().is_empty()
            && self.runtime.core_instance_id.len() <= 256
            && !self.profile_id.trim().is_empty()
            && self.profile_id.len() <= 512
            && self.client_generation > 0
    }
}

/// Not serializable or constructible by a guest. The host supplies the verified
/// native Window.label(), never a label from an invoke argument.
pub struct HostCaller;
impl HostCaller {
    pub fn from_window_label(verified_label: &str) -> Result<Self, IssueError> {
        if verified_label != "main" {
            return Err(IssueError::Forbidden);
        }
        Ok(Self)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueError {
    Forbidden,
    Unavailable,
    Busy,
    InvalidBinding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReceiptState {
    Pending,
    Confirmed,
    Rejected,
    Unknown,
    Invalidated,
    Denied,
    DeliveryDenied,
    NotSubmitted,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationReceipt {
    pub operation_id: String,
    pub core_instance_id: String,
    pub profile_id: String,
    pub config_revision: String,
    pub client_generation: String,
    pub mutation: bool,
    pub state: ReceiptState,
}
struct State {
    current: Option<RuntimeBinding>,
    epoch: u64,
    next: u64,
    receipts: VecDeque<OperationReceipt>,
}
#[derive(Clone)]
pub struct RuntimeBridge {
    state: Arc<Mutex<State>>,
}
impl Default for RuntimeBridge {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                current: None,
                epoch: 0,
                next: 0,
                receipts: VecDeque::new(),
            })),
        }
    }
}

/// Single-use, opaque ticket. No Clone/Deserialize implementation or public ID
/// constructor exists. Dropping an issued ticket cannot replay any command.
pub struct OperationTicket {
    owner: Arc<Mutex<State>>,
    binding: RuntimeBinding,
    id: String,
    epoch: u64,
    started: bool,
}
impl OperationTicket {
    pub fn operation_id(&self) -> &str {
        &self.id
    }
    pub fn binding(&self) -> &RuntimeBinding {
        &self.binding
    }
    fn finish(&self, state: ReceiptState, mutation: bool) {
        let mut owner = self.owner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(receipt) = owner
            .receipts
            .iter_mut()
            .find(|receipt| receipt.operation_id == self.id)
        {
            receipt.state = state;
            receipt.mutation = mutation;
        }
    }
}
impl Drop for OperationTicket {
    fn drop(&mut self) {
        let mut owner = self.owner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(receipt) = owner.receipts.iter_mut().find(|receipt| {
            receipt.operation_id == self.id && receipt.state == ReceiptState::Pending
        }) {
            receipt.state = if self.started {
                ReceiptState::Unknown
            } else {
                ReceiptState::NotSubmitted
            };
        }
    }
}

pub enum BridgeRequest {
    Query(QueryRequest),
    Command(CommandRequest),
}
#[derive(Debug)]
pub enum BridgeError<E> {
    Unavailable,
    Forbidden,
    StaleBinding,
    Unsupported,
    InvalidResponse,
    Capability(CapabilityError),
    Transport(E),
    Rejected(E),
    Unconfirmed,
}
pub struct BridgeResult {
    pub receipt: OperationReceipt,
    pub value: Value,
}

impl RuntimeBridge {
    /// Only a trusted owner may install facts read from its registered runtime.
    /// Android/iOS must leave this None until a real Proxy owner exists; an
    /// EngineHandle, UI flag, endpoint path, or claimed version is not evidence.
    pub fn bind(&self, binding: RuntimeBinding) -> Result<(), IssueError> {
        if !binding.valid() {
            return Err(IssueError::InvalidBinding);
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.current.as_ref() != Some(&binding) {
            state.epoch = state.epoch.checked_add(1).ok_or(IssueError::Busy)?;
            state.current = Some(binding);
        }
        Ok(())
    }
    pub fn invalidate(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.current = None;
        state.epoch = state.epoch.saturating_add(1);
    }
    pub fn binding(&self) -> Option<RuntimeBinding> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .current
            .clone()
    }
    pub fn issue(&self, _caller: &HostCaller) -> Result<OperationTicket, IssueError> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let binding = state.current.clone().ok_or(IssueError::Unavailable)?;
        if state.receipts.len() == MAX_RECEIPTS {
            let index = state
                .receipts
                .iter()
                .position(|receipt| receipt.state != ReceiptState::Pending)
                .ok_or(IssueError::Busy)?;
            state.receipts.remove(index);
        }
        state.next = state.next.checked_add(1).ok_or(IssueError::Busy)?;
        let id = state.next.to_string();
        state.receipts.push_back(OperationReceipt {
            operation_id: id.clone(),
            core_instance_id: binding.runtime.core_instance_id.clone(),
            profile_id: binding.profile_id.clone(),
            config_revision: binding.runtime.config_revision.to_string(),
            client_generation: binding.client_generation.to_string(),
            mutation: false,
            state: ReceiptState::Pending,
        });
        Ok(OperationTicket {
            owner: self.state.clone(),
            binding,
            id,
            epoch: state.epoch,
            started: false,
        })
    }
    pub fn receipt(&self, _caller: &HostCaller, operation_id: &str) -> Option<OperationReceipt> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .receipts
            .iter()
            .find(|receipt| receipt.operation_id == operation_id)
            .cloned()
    }
    fn current(&self, ticket: &OperationTicket) -> bool {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        Arc::ptr_eq(&self.state, &ticket.owner)
            && state.epoch == ticket.epoch
            && state.current.as_ref() == Some(&ticket.binding)
    }

    /// Queries and two ID-scoped controls consume the same capability manager
    /// used by existing client workflows. Other controls remain with their
    /// domain owners; this bridge is never an arbitrary invoke/command API.
    pub async fn call<T: BridgeTransport>(
        &self,
        transport: &T,
        mut ticket: OperationTicket,
        request: BridgeRequest,
        lease: &Lease,
        kernel_auth: &AuthContext,
    ) -> Result<BridgeResult, BridgeError<T::Error>> {
        let mutation = matches!(&request, BridgeRequest::Command(_));
        if !self.current(&ticket) {
            ticket.finish(ReceiptState::Invalidated, mutation);
            return Err(BridgeError::StaleBinding);
        }
        let required = match &request {
            BridgeRequest::Query(QueryRequest::Unknown(_) | QueryRequest::Config(_)) => {
                ticket.finish(ReceiptState::Denied, mutation);
                return Err(BridgeError::Unsupported);
            }
            BridgeRequest::Query(request) => request.required_permission(),
            BridgeRequest::Command(CommandRequest::FlowClose(command))
                if !command.flow_id.is_empty() && command.flow_id.len() <= 512 =>
            {
                zero_api::Permission::Control
            }
            BridgeRequest::Command(CommandRequest::PacketRouteClose(command))
                if !command.route_id.is_empty()
                    && command.expected_core_instance_id
                        == ticket.binding.runtime.core_instance_id
                    && command.expected_config_revision
                        == Some(ticket.binding.runtime.config_revision) =>
            {
                zero_api::Permission::Admin
            }
            BridgeRequest::Command(_) => {
                ticket.finish(ReceiptState::Denied, mutation);
                return Err(BridgeError::Unsupported);
            }
        };
        if lease.identity() != HOST_IDENTITY || !kernel_auth.allows(required) {
            ticket.finish(ReceiptState::Denied, mutation);
            return Err(BridgeError::Forbidden);
        }
        let permission = Permission::new(
            if mutation {
                "runtime.control"
            } else {
                "runtime.query"
            },
            ticket.binding.permission_scope(),
        );
        let remaining = lease.remaining().map_err(|error| {
            ticket.finish(ReceiptState::Denied, mutation);
            BridgeError::Capability(error)
        })?;
        let mut cause = None;
        let mut submitted = false;
        let expected_target = match &request {
            BridgeRequest::Command(CommandRequest::FlowClose(command)) => {
                Some((false, command.flow_id.clone()))
            }
            BridgeRequest::Command(CommandRequest::PacketRouteClose(command)) => {
                Some((true, command.route_id.clone()))
            }
            _ => None,
        };
        ticket.started = true;
        let resource = lease
            .execute_async(&permission, async {
                let run = async {
                    let before = transport
                        .query(json!({"runtime":{}}))
                        .await
                        .map_err(BridgeError::Transport)?;
                    if !same_runtime(&before, &ticket.binding.runtime) || !self.current(&ticket) {
                        return Err(BridgeError::StaleBinding);
                    }
                    lease
                        .check(Some(&permission))
                        .map_err(BridgeError::Capability)?;
                    submitted = true;
                    let value = match request {
                        BridgeRequest::Query(request) => transport
                            .query(
                                serde_json::to_value(request)
                                    .map_err(|_| BridgeError::InvalidResponse)?,
                            )
                            .await
                            .map_err(BridgeError::Transport),
                        BridgeRequest::Command(command) => {
                            transport.command(command).await.map_err(|error| {
                                if transport.failure_is_rejection(&error) {
                                    BridgeError::Rejected(error)
                                } else {
                                    BridgeError::Transport(error)
                                }
                            })
                        }
                    }?;
                    let after = transport
                        .query(json!({"runtime":{}}))
                        .await
                        .map_err(BridgeError::Transport)?;
                    if !same_runtime(&after, &ticket.binding.runtime) || !self.current(&ticket) {
                        return Err(BridgeError::StaleBinding);
                    }
                    let bytes = serde_json::to_vec(&value)
                        .map_err(|_| BridgeError::InvalidResponse)?
                        .len();
                    if bytes > MAX_RESULT_BYTES {
                        return Err(BridgeError::Capability(CapabilityError::BudgetExceeded));
                    }
                    Ok((value, bytes))
                };
                match tokio::time::timeout(remaining, run).await {
                    Ok(Ok(result)) => Ok(result),
                    Ok(Err(error)) => {
                        cause = Some(error);
                        Err(CapabilityError::Transport)
                    }
                    Err(_) => {
                        cause = Some(BridgeError::Unconfirmed);
                        Err(CapabilityError::Deadline)
                    }
                }
            })
            .await;
        let value = match resource.and_then(|resource| resource.take(lease)) {
            Ok(value) => value,
            Err(error) => {
                let error = if matches!(
                    error,
                    CapabilityError::Transport | CapabilityError::Deadline
                ) {
                    cause.unwrap_or(BridgeError::Capability(error))
                } else {
                    BridgeError::Capability(error)
                };
                ticket.finish(
                    match &error {
                        BridgeError::StaleBinding => ReceiptState::Invalidated,
                        BridgeError::Rejected(_) => ReceiptState::Rejected,
                        BridgeError::Capability(
                            CapabilityError::PermissionDenied
                            | CapabilityError::Revoked
                            | CapabilityError::Cancelled,
                        ) => {
                            if submitted {
                                ReceiptState::DeliveryDenied
                            } else {
                                ReceiptState::Denied
                            }
                        }
                        _ if mutation && !submitted => ReceiptState::NotSubmitted,
                        _ if mutation => ReceiptState::Unknown,
                        _ => ReceiptState::Rejected,
                    },
                    mutation,
                );
                return Err(error);
            }
        };
        if !self.current(&ticket) {
            ticket.finish(ReceiptState::Invalidated, mutation);
            return Err(BridgeError::StaleBinding);
        }
        if mutation && value.get("accepted").and_then(Value::as_bool).is_none() {
            ticket.finish(ReceiptState::Unknown, true);
            return Err(BridgeError::Unconfirmed);
        }
        let accepted = !mutation || value.get("accepted").and_then(Value::as_bool) == Some(true);
        if accepted
            && expected_target.as_ref().is_some_and(|(packet, target)| {
                let result = value.get("result");
                if result
                    .and_then(|value| value.get("closed"))
                    .and_then(Value::as_bool)
                    != Some(true)
                {
                    return true;
                }
                if *packet {
                    result
                        .and_then(|value| value.pointer("/route/route_id"))
                        .and_then(Value::as_str)
                        != Some(target.as_str())
                        || result
                            .and_then(|value| value.pointer("/route/core_instance_id"))
                            .and_then(Value::as_str)
                            != Some(ticket.binding.runtime.core_instance_id.as_str())
                        || result
                            .and_then(|value| value.pointer("/route/config_revision"))
                            .and_then(Value::as_u64)
                            != Some(ticket.binding.runtime.config_revision)
                } else {
                    result
                        .and_then(|value| value.get("flow_id"))
                        .and_then(Value::as_str)
                        != Some(target.as_str())
                }
            })
        {
            ticket.finish(ReceiptState::Unknown, true);
            return Err(BridgeError::Unconfirmed);
        }
        ticket.finish(
            if accepted {
                ReceiptState::Confirmed
            } else {
                ReceiptState::Rejected
            },
            mutation,
        );
        Ok(BridgeResult {
            receipt: self
                .receipt(&HostCaller, &ticket.id)
                .expect("active receipt retained"),
            value,
        })
    }
}

fn same_runtime(value: &Value, expected: &RuntimeIdentity) -> bool {
    let runtime = value.get("runtime").unwrap_or(value);
    runtime
        .get("core_instance_id")
        .or_else(|| runtime.get("coreInstanceId"))
        .and_then(Value::as_str)
        == Some(expected.core_instance_id.as_str())
        && runtime
            .get("config_revision")
            .or_else(|| runtime.get("configRevision"))
            .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
            == Some(expected.config_revision)
}

pub mod events;
#[cfg(test)]
mod tests;
