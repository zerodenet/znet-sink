use super::*;

// Registry entries hold policy data, never a Manager, avoiding an ownership cycle.
#[derive(Clone)]
pub(in crate::capability) struct StoredPolicy {
    identity: String,
    declared: BTreeSet<Permission>,
    required: BTreeSet<Permission>,
    state: Arc<Mutex<State>>,
}
impl Policy {
    pub(in crate::capability) fn stored(&self) -> StoredPolicy {
        StoredPolicy {
            identity: self.identity.clone(),
            declared: self.declared.clone(),
            required: self.required.clone(),
            state: self.state.clone(),
        }
    }
}
impl StoredPolicy {
    pub(in crate::capability) fn restore(&self, manager: &Manager) -> Policy {
        Policy {
            manager: manager.clone(),
            identity: self.identity.clone(),
            declared: self.declared.clone(),
            required: self.required.clone(),
            state: self.state.clone(),
        }
    }
    pub(in crate::capability) fn matches(&self, other: &Self) -> bool {
        self.identity == other.identity
            && self.declared == other.declared
            && self.required == other.required
    }
    pub(in crate::capability) fn idle_retired(&self) -> bool {
        let s = self.state.lock().unwrap();
        s.retired && !s.busy
    }
    pub(in crate::capability) fn retired(&self) -> bool {
        self.state.lock().unwrap().retired
    }
    pub(in crate::capability) fn retire(&self) -> bool {
        let mut s = self.state.lock().unwrap();
        s.retired = true;
        s.enabled = false;
        s.epoch += 1;
        s.grants.clear();
        s.busy
    }
    pub(in crate::capability) fn snapshot(
        &self,
        key: &str,
        registration: u64,
    ) -> crate::capability::ComponentSnapshot {
        let s = self.state.lock().unwrap();
        crate::capability::ComponentSnapshot {
            key: key.into(),
            identity: self.identity.clone(),
            declared: self.declared.iter().cloned().collect(),
            required: self.required.iter().cloned().collect(),
            grants: s.grants.iter().cloned().collect(),
            enabled: s.enabled && s.expires > Instant::now(),
            retired: s.retired,
            running: s.busy,
            revision: s.epoch,
            registration,
        }
    }
}
