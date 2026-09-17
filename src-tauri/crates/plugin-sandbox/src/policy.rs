//! Component admission adapter. Invocation policy and lifecycle belong to client-core.
use crate::contract::{Component, Error, Request};
use std::{
    collections::BTreeSet,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use znet_client_core::capability::{self, Budget, Manager, Permission, Policy};

#[derive(Clone)]
pub struct Authority {
    digest: String,
    policy: Policy,
}
impl Authority {
    /// Standalone lab convenience. Production adapters must inject their client manager.
    pub fn admit(component: &Component, ceiling: &BTreeSet<Request>) -> Result<Self, Error> {
        Self::admit_with_manager(component, ceiling, &Manager::default())
    }
    pub fn admit_with_manager(
        component: &Component,
        ceiling: &BTreeSet<Request>,
        manager: &Manager,
    ) -> Result<Self, Error> {
        let declared = component
            .manifest
            .required
            .iter()
            .chain(&component.manifest.optional)
            .map(permission)
            .collect();
        let required = component.manifest.required.iter().map(permission).collect();
        let ceiling = ceiling.iter().map(permission).collect();
        let policy = manager
            .admit_component(
                format!(
                    "{}/{}",
                    component.manifest.plugin_id, component.manifest.component_id
                ),
                component.digest.clone(),
                declared,
                required,
                &ceiling,
            )
            .map_err(map_error)?;
        Ok(Self {
            digest: component.digest.clone(),
            policy,
        })
    }
    pub fn authorize(&self, grants: BTreeSet<Request>, ttl: Duration) -> Result<(), Error> {
        self.policy
            .authorize(grants.iter().map(permission).collect(), ttl)
            .map_err(map_error)
    }
    pub fn revoke(&self) {
        self.policy.revoke();
    }
    pub fn begin(&self, component: &Component) -> Result<Lease, Error> {
        self.begin_cancelled(component, Arc::new(AtomicBool::new(false)))
    }
    pub(crate) fn begin_cancelled(
        &self,
        component: &Component,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Lease, Error> {
        if component.digest != self.digest {
            return Err(Error::DigestMismatch);
        }
        let limits = &component.manifest.limits;
        let inner = self
            .policy
            .begin(
                Budget {
                    calls: limits.max_calls,
                    resource_bytes: limits.output_bytes,
                    timeout: Duration::from_millis(limits.timeout_ms),
                },
                cancelled,
            )
            .map_err(map_error)?;
        Ok(Lease { inner })
    }
}
pub struct Lease {
    pub(crate) inner: capability::Lease,
}
impl Lease {
    pub fn check(&self, request: Option<&Request>) -> Result<(), Error> {
        self.inner
            .check(request.map(permission).as_ref())
            .map_err(map_error)
    }

    /// Native host capability adapters only. Guest code never receives the
    /// underlying lease or a way to construct permissions.
    pub fn host_lease(&self) -> &capability::Lease {
        &self.inner
    }
}
pub(crate) fn permission(request: &Request) -> Permission {
    Permission::new(request.capability.as_str(), &request.scope)
}
pub(crate) fn map_error(error: capability::Error) -> Error {
    match error {
        capability::Error::AdmissionDenied => Error::AdmissionDenied,
        capability::Error::PermissionDenied => Error::PermissionDenied,
        capability::Error::Disabled => Error::Disabled,
        capability::Error::Busy => Error::Busy,
        capability::Error::Revoked => Error::Revoked,
        capability::Error::Expired => Error::Expired,
        capability::Error::Deadline => Error::Deadline,
        capability::Error::Cancelled => Error::Cancelled,
        capability::Error::BudgetExceeded => Error::BudgetExceeded,
        capability::Error::InvalidRequest => Error::InvalidOutput,
        capability::Error::Transport => Error::HostFailure,
    }
}
