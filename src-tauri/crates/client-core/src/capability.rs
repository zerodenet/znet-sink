//! Host-owned admission, invocation and resource lifecycle shared by all adapters.
//! Native callers are trusted to establish policy; these constructors are never guest APIs.
mod components;
mod operation;
mod policy;
mod resource;
pub use components::ComponentSnapshot;
pub use operation::{Claim, Manager, Operation, OperationState};
pub use policy::{Budget, Lease, Policy};
pub use resource::Resource;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    AdmissionDenied,
    PermissionDenied,
    Disabled,
    Busy,
    Revoked,
    Expired,
    Deadline,
    Cancelled,
    BudgetExceeded,
    InvalidRequest,
    Transport,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct Permission {
    pub capability: String,
    pub scope: String,
}
impl Permission {
    pub fn new(capability: impl Into<String>, scope: impl Into<String>) -> Self {
        Self {
            capability: capability.into(),
            scope: scope.into(),
        }
    }
}
fn allows(grants: &BTreeSet<Permission>, request: &Permission) -> bool {
    grants.contains(request) || grants.contains(&Permission::new(&request.capability, "*"))
}
