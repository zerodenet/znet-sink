//! Manual probe execution resources belong to one client instance.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::client_core::ProbeJobId;

pub const MAX_CONCURRENT_PROBES: usize = 8;

pub(crate) struct ProbeRuntime {
    concurrency: Arc<tokio::sync::Semaphore>,
    operations: Mutex<HashMap<(ProbeJobId, String), String>>,
}

impl Default for ProbeRuntime {
    fn default() -> Self {
        Self {
            concurrency: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_PROBES)),
            operations: Mutex::new(HashMap::new()),
        }
    }
}

impl ProbeRuntime {
    pub(crate) fn semaphore(&self) -> Arc<tokio::sync::Semaphore> {
        self.concurrency.clone()
    }

    pub(crate) fn remember(&self, job: ProbeJobId, target: &str, operation: String) {
        self.operations
            .lock()
            .expect("probe operations poisoned")
            .insert((job, target.to_owned()), operation);
    }

    pub(crate) fn expected(&self, job: ProbeJobId, target: &str) -> Option<String> {
        self.operations
            .lock()
            .expect("probe operations poisoned")
            .get(&(job, target.to_owned()))
            .cloned()
    }

    pub(crate) fn forget(&self, job: ProbeJobId, target: &str) {
        self.operations
            .lock()
            .expect("probe operations poisoned")
            .remove(&(job, target.to_owned()));
    }

    pub(crate) fn forget_job(&self, job: ProbeJobId) {
        self.operations
            .lock()
            .expect("probe operations poisoned")
            .retain(|(id, _), _| *id != job);
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
