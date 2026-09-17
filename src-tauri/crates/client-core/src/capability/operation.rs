use super::{Error, Permission, Policy};
use std::{
    collections::{BTreeSet, VecDeque},
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationState {
    Running,
    Completed,
    Failed(Error),
    /// Execution happened, but its result cannot be delivered. This is not rollback.
    DeliveryDenied(Error),
}
#[derive(Debug, Clone)]
pub struct Operation {
    pub id: u64,
    pub parent: u64,
    pub capability: String,
    pub state: OperationState,
}
struct State {
    next: u64,
    active: usize,
    operations: VecDeque<Operation>,
    claims: BTreeSet<String>,
    closed: bool,
}
#[derive(Clone)]
pub struct Manager {
    pub(super) components: Arc<Mutex<super::components::Registry>>,
    state: Arc<Mutex<State>>,
}
impl Default for Manager {
    fn default() -> Self {
        Self {
            components: Arc::new(Mutex::new(super::components::Registry::default())),
            state: Arc::new(Mutex::new(State {
                next: 0,
                active: 0,
                operations: VecDeque::new(),
                claims: BTreeSet::new(),
                closed: false,
            })),
        }
    }
}
impl Manager {
    pub fn admit(
        &self,
        identity: String,
        declared: BTreeSet<Permission>,
        required: BTreeSet<Permission>,
        ceiling: &BTreeSet<Permission>,
    ) -> Result<Policy, Error> {
        self.ensure_open()?;
        if identity.is_empty()
            || !required.is_subset(&declared)
            || !declared.iter().all(|r| super::allows(ceiling, r))
        {
            return Err(Error::AdmissionDenied);
        }
        Ok(Policy::new(self.clone(), identity, declared, required))
    }
    pub(super) fn claim(&self, key: String) -> Result<Claim, Error> {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return Err(Error::Cancelled);
        }
        if state.claims.len() >= 32 || !state.claims.insert(key.clone()) {
            return Err(Error::Busy);
        }
        Ok(Claim {
            manager: self.clone(),
            key,
        })
    }
    pub fn operations(&self) -> Vec<Operation> {
        self.state
            .lock()
            .unwrap()
            .operations
            .iter()
            .cloned()
            .collect()
    }
    pub(super) fn next_id(&self) -> u64 {
        let mut s = self.state.lock().unwrap();
        s.next += 1;
        s.next
    }
    pub(super) fn start(
        &self,
        parent: u64,
        permission: &Permission,
    ) -> Result<OperationGuard, Error> {
        let mut s = self.state.lock().unwrap();
        if s.closed {
            return Err(Error::Cancelled);
        }
        if s.active >= 32 {
            return Err(Error::Busy);
        }
        if s.operations.len() == 256 {
            let index = s
                .operations
                .iter()
                .position(|o| o.state != OperationState::Running)
                .ok_or(Error::Busy)?;
            s.operations.remove(index);
        }
        s.next += 1;
        let id = s.next;
        s.active += 1;
        s.operations.push_back(Operation {
            id,
            parent,
            capability: permission.capability.clone(),
            state: OperationState::Running,
        });
        Ok(OperationGuard {
            manager: self.clone(),
            id,
            finished: false,
        })
    }
    pub(super) fn ensure_open(&self) -> Result<(), Error> {
        if self.state.lock().unwrap().closed {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Close every authority owned by this client lifetime. Existing leases
    /// fail their next check and no native or guest caller can be admitted.
    pub fn shutdown(&self) {
        self.shutdown_components();
        self.state.lock().unwrap().closed = true;
    }
}
pub(super) struct OperationGuard {
    manager: Manager,
    id: u64,
    finished: bool,
}
impl OperationGuard {
    pub fn finish(&mut self, state: OperationState) {
        let mut s = self.manager.state.lock().unwrap();
        if let Some(o) = s.operations.iter_mut().find(|o| o.id == self.id) {
            o.state = state;
        }
        s.active -= 1;
        self.finished = true;
    }
}
impl Drop for OperationGuard {
    fn drop(&mut self) {
        if !self.finished {
            self.finish(OperationState::DeliveryDenied(Error::Cancelled));
        }
    }
}

/// Exclusive execution ownership, without holding a mutex across IO/await.
pub struct Claim {
    manager: Manager,
    key: String,
}
impl Drop for Claim {
    fn drop(&mut self) {
        self.manager.state.lock().unwrap().claims.remove(&self.key);
    }
}
