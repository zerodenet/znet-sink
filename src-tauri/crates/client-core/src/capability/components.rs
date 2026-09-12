//! Host-owned component identity and lifecycle. Native operations still use admit;
//! component adapters must use admit_component to retain one authorization state.
use super::{policy::StoredPolicy, Error, Manager, Permission, Policy};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct Registry {
    entries: BTreeMap<String, Registered>,
    closed: bool,
}
struct Registered {
    policy: StoredPolicy,
    registration: u64,
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct ComponentSnapshot {
    pub key: String,
    pub identity: String,
    pub declared: Vec<Permission>,
    pub required: Vec<Permission>,
    pub grants: Vec<Permission>,
    pub enabled: bool,
    pub retired: bool,
    pub running: bool,
    pub revision: u64,
    pub registration: u64,
}
impl Manager {
    /// Repeated admission retains grants, revocation and the single running lease.
    /// Changed material retires the old authority and starts without any grants.
    /// A retiring invocation must exit before its replacement can be admitted.
    pub fn admit_component(
        &self,
        key: String,
        identity: String,
        declared: BTreeSet<Permission>,
        required: BTreeSet<Permission>,
        ceiling: &BTreeSet<Permission>,
    ) -> Result<Policy, Error> {
        if key.is_empty() || key.len() > 256 || identity.len() > 256 {
            return Err(Error::AdmissionDenied);
        }
        let candidate = self.admit(identity, declared, required, ceiling)?;
        let stored = candidate.stored();
        let mut registry = self.components.lock().unwrap();
        if registry.closed {
            return Err(Error::Disabled);
        }
        registry
            .entries
            .retain(|_, policy| !policy.policy.idle_retired());
        if let Some(previous) = registry.entries.get(&key) {
            if previous.policy.matches(&stored) && !previous.policy.retired() {
                return Ok(previous.policy.restore(self));
            }
            if previous.policy.retire() {
                return Err(Error::Busy);
            }
        } else if registry.entries.len() >= 64 {
            return Err(Error::BudgetExceeded);
        }
        registry.entries.insert(
            key,
            Registered {
                policy: stored,
                registration: self.next_id(),
            },
        );
        Ok(candidate)
    }
    pub fn component_snapshots(&self) -> Vec<ComponentSnapshot> {
        self.components
            .lock()
            .unwrap()
            .entries
            .iter()
            .map(|(key, entry)| entry.policy.snapshot(key, entry.registration))
            .collect()
    }
    /// Apply a host UI decision only to the exact material and review revision.
    /// An old dialog cannot re-enable a revoked or upgraded component.
    pub fn authorize_component(
        &self,
        review: &ComponentSnapshot,
        grants: BTreeSet<Permission>,
        ttl: std::time::Duration,
    ) -> Result<(), Error> {
        let registry = self.components.lock().unwrap();
        if registry.closed {
            return Err(Error::Disabled);
        }
        let stored = registry
            .entries
            .get(&review.key)
            .ok_or(Error::AdmissionDenied)?;
        let policy = stored.policy.restore(self);
        if policy.identity() != review.identity || stored.registration != review.registration {
            return Err(Error::Revoked);
        }
        policy.authorize_revision(grants, ttl, Some(review.revision))
    }
    /// Stop access immediately. Explicit host authorization may enable it again.
    pub fn revoke_component(&self, key: &str) -> bool {
        let registry = self.components.lock().unwrap();
        let Some(policy) = registry.entries.get(key) else {
            return false;
        };
        policy.policy.restore(self).revoke();
        true
    }
    /// Permanently retire existing handles. Retain a busy tombstone until the
    /// invocation exits so remove/reinstall cannot overlap the old VM.
    pub fn remove_component(&self, key: &str) -> bool {
        let mut registry = self.components.lock().unwrap();
        let Some(policy) = registry.entries.get(key) else {
            return false;
        };
        if !policy.policy.retire() {
            registry.entries.remove(key);
        }
        true
    }
    /// Application shutdown denies both existing handles and future admission.
    pub fn shutdown_components(&self) {
        let mut registry = self.components.lock().unwrap();
        registry.closed = true;
        for policy in registry.entries.values() {
            policy.policy.retire();
        }
    }
}
