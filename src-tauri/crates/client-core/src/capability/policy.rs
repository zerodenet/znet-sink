mod stored;
use super::{allows, Error, Manager, OperationState, Permission, Resource};
use std::{
    collections::BTreeSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
const MAX_HOST_RESOURCE_BYTES: usize = 512 * 1024 * 1024 + 8192;
const MAX_HOST_OPERATION_TIMEOUT: Duration = Duration::from_secs(600);
pub(super) use stored::StoredPolicy;

pub(super) struct State {
    enabled: bool,
    retired: bool,
    epoch: u64,
    busy: bool,
    grants: BTreeSet<Permission>,
    /// Temporary callers use a deadline; host-owned component consent is
    /// durable and is restored explicitly after every application start.
    expires: Option<Instant>,
}
#[derive(Clone)]
pub struct Policy {
    pub(super) manager: Manager,
    identity: String,
    declared: BTreeSet<Permission>,
    required: BTreeSet<Permission>,
    state: Arc<Mutex<State>>,
}
#[derive(Clone)]
pub struct Budget {
    pub calls: u32,
    pub resource_bytes: usize,
    pub timeout: Duration,
}
impl Policy {
    pub(super) fn new(
        manager: Manager,
        identity: String,
        declared: BTreeSet<Permission>,
        required: BTreeSet<Permission>,
    ) -> Self {
        Self {
            manager,
            identity,
            declared,
            required,
            state: Arc::new(Mutex::new(State {
                enabled: false,
                retired: false,
                epoch: 0,
                busy: false,
                grants: BTreeSet::new(),
                expires: Some(Instant::now()),
            })),
        }
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn authorize(&self, grants: BTreeSet<Permission>, ttl: Duration) -> Result<(), Error> {
        self.authorize_revision(grants, Some(ttl), None)
    }
    pub(super) fn authorize_revision(
        &self,
        grants: BTreeSet<Permission>,
        ttl: Option<Duration>,
        revision: Option<u64>,
    ) -> Result<(), Error> {
        self.manager.ensure_open()?;
        if !grants.is_subset(&self.declared)
            || ttl.is_some_and(|ttl| ttl.is_zero() || ttl > Duration::from_secs(3600))
        {
            return Err(Error::PermissionDenied);
        }
        let mut s = self.state.lock().unwrap();
        if s.retired || revision.is_some_and(|revision| revision != s.epoch) {
            return Err(Error::Revoked);
        }
        if ttl.is_none() && s.enabled && s.expires.is_none() && s.grants == grants {
            return Ok(());
        }
        s.epoch += 1;
        s.enabled = true;
        s.grants = grants;
        s.expires = ttl.map(|ttl| Instant::now() + ttl);
        Ok(())
    }
    pub fn revoke(&self) {
        let mut s = self.state.lock().unwrap();
        s.epoch += 1;
        s.enabled = false;
        s.grants.clear();
    }
    pub fn begin(&self, budget: Budget, cancelled: Arc<AtomicBool>) -> Result<Lease, Error> {
        self.manager.ensure_open()?;
        if budget.calls == 0
            || budget.calls > 256
            || budget.resource_bytes == 0
            || budget.resource_bytes > MAX_HOST_RESOURCE_BYTES
            || budget.timeout.is_zero()
            || budget.timeout > MAX_HOST_OPERATION_TIMEOUT
        {
            return Err(Error::BudgetExceeded);
        }
        let mut s = self.state.lock().unwrap();
        if !s.enabled {
            return Err(Error::Disabled);
        }
        if s.expires.is_some_and(|expires| expires <= Instant::now()) {
            return Err(Error::Expired);
        }
        if !self.required.is_subset(&s.grants) {
            return Err(Error::PermissionDenied);
        }
        if s.busy {
            return Err(Error::Busy);
        }
        s.busy = true;
        Ok(Lease {
            policy: self.clone(),
            epoch: s.epoch,
            id: self.manager.next_id(),
            deadline: Instant::now() + budget.timeout,
            budget,
            cancelled,
            usage: Arc::new(Mutex::new((0, 0))),
        })
    }
}
pub struct Lease {
    policy: Policy,
    epoch: u64,
    id: u64,
    deadline: Instant,
    budget: Budget,
    cancelled: Arc<AtomicBool>,
    pub(super) usage: Arc<Mutex<(u32, usize)>>,
}
impl Lease {
    pub fn check(&self, permission: Option<&Permission>) -> Result<(), Error> {
        self.policy.manager.ensure_open()?;
        let s = self.policy.state.lock().unwrap();
        if !s.enabled || s.epoch != self.epoch {
            return Err(Error::Revoked);
        }
        if s.expires.is_some_and(|expires| expires <= Instant::now()) {
            return Err(Error::Expired);
        }
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        if Instant::now() >= self.deadline {
            return Err(Error::Deadline);
        }
        if permission.is_some_and(|p| !allows(&s.grants, p)) {
            return Err(Error::PermissionDenied);
        }
        Ok(())
    }
    pub fn remaining(&self) -> Result<Duration, Error> {
        self.check(None)?;
        Ok(self.deadline.saturating_duration_since(Instant::now()))
    }
    pub fn identity(&self) -> &str {
        self.policy.identity()
    }
    pub fn claim_resource(&self, key: String) -> Result<super::Claim, Error> {
        self.check(None)?;
        self.policy.manager.claim(key)
    }
    pub fn execute<T>(
        &self,
        permission: &Permission,
        run: impl FnOnce() -> Result<(T, usize), Error>,
    ) -> Result<Resource<T>, Error> {
        let operation = self.start_operation(permission)?;
        self.complete(permission, operation, run())
    }
    pub async fn execute_async<T>(
        &self,
        permission: &Permission,
        run: impl std::future::Future<Output = Result<(T, usize), Error>>,
    ) -> Result<Resource<T>, Error> {
        let operation = self.start_operation(permission)?;
        self.complete(permission, operation, run.await)
    }
    fn start_operation(
        &self,
        permission: &Permission,
    ) -> Result<super::operation::OperationGuard, Error> {
        self.check(Some(permission))?;
        {
            let mut usage = self.usage.lock().unwrap();
            if usage.0 >= self.budget.calls {
                return Err(Error::BudgetExceeded);
            }
            usage.0 += 1;
        }
        self.policy.manager.start(self.id, permission)
    }
    fn complete<T>(
        &self,
        permission: &Permission,
        mut operation: super::operation::OperationGuard,
        result: Result<(T, usize), Error>,
    ) -> Result<Resource<T>, Error> {
        if let Err(error) = self.check(Some(permission)) {
            operation.finish(OperationState::DeliveryDenied(error));
            return Err(error);
        }
        match result {
            Err(error) => {
                operation.finish(OperationState::Failed(error));
                Err(error)
            }
            Ok((value, bytes)) => {
                let mut usage = self.usage.lock().unwrap();
                if bytes > self.budget.resource_bytes.saturating_sub(usage.1) {
                    operation.finish(OperationState::DeliveryDenied(Error::BudgetExceeded));
                    return Err(Error::BudgetExceeded);
                }
                usage.1 += bytes;
                operation.finish(OperationState::Completed);
                Ok(Resource::new(value, bytes, self.usage.clone()))
            }
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.policy.state.lock().unwrap().busy = false;
    }
}
