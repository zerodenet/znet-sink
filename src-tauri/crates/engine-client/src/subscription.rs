use serde::Serialize;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub transport: &'static str,
    pub path: String,
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub endpoint: Endpoint,
    pub timeout: Duration,
}

/// Owns the selected observation endpoint and cancellation generation together.
/// Stopping a subscription returns only its endpoint, never an unrelated one.
#[derive(Default)]
pub struct SubscriptionOwner {
    generation: Arc<AtomicU64>,
    binding: Mutex<Option<Binding>>,
}

#[derive(Clone)]
pub struct Subscription {
    pub generation: u64,
    pub binding: Binding,
    active: Arc<AtomicU64>,
}

impl SubscriptionOwner {
    pub fn begin(&self, binding: Binding) -> Subscription {
        let mut selected = self.binding.lock().unwrap_or_else(|e| e.into_inner());
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        *selected = Some(binding.clone());
        Subscription {
            generation,
            binding,
            active: self.generation.clone(),
        }
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == generation
    }

    pub fn binding(&self) -> Option<Binding> {
        self.binding
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn stop(&self) -> (u64, Option<Binding>) {
        let mut selected = self.binding.lock().unwrap_or_else(|e| e.into_inner());
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        (generation, selected.take())
    }
}

impl Subscription {
    pub fn is_current(&self) -> bool {
        self.active.load(Ordering::SeqCst) == self.generation
    }
}
