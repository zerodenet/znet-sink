//! Bounded public invalidation hints. Full payloads/configuration/credentials
//! never enter this buffer; domain observations keep their existing owners.
use super::{HostCaller, RuntimeBinding, RuntimeBridge};
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use zero_api::RawApiEvent;

const CAPACITY: usize = 128;
const PAGE_LIMIT: usize = 64;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventHint {
    pub event_type: String,
    pub sequence: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HintReplay {
    pub core_instance_id: String,
    pub config_revision: String,
    pub profile_id: String,
    pub client_generation: String,
    pub requested_after: String,
    pub current_sequence: String,
    pub has_gap: bool,
    pub events: Vec<EventHint>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenerError {
    Unavailable,
    StaleBinding,
    Cancelled,
    InvalidBudget,
}
struct BufferState {
    binding: RuntimeBinding,
    epoch: u64,
    events: VecDeque<(u64, EventHint)>,
    latest: u64,
    evicted_through: u64,
    lost: bool,
}
pub struct EventBuffer {
    bridge: RuntimeBridge,
    state: Arc<Mutex<Option<BufferState>>>,
}
/// Cancelling/dropping this listener affects delivery only. It owns no runtime,
/// transport, native tunnel, shared event source, or business task.
pub struct Listener {
    owner: Arc<Mutex<Option<BufferState>>>,
    binding: RuntimeBinding,
    epoch: u64,
    active: AtomicBool,
}
impl Listener {
    pub fn cancel(&self) {
        self.active.store(false, Ordering::Release);
    }
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
}
impl Drop for Listener {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl EventBuffer {
    pub fn new(bridge: RuntimeBridge) -> Self {
        Self {
            bridge,
            state: Arc::new(Mutex::new(None)),
        }
    }
    /// A trusted owner calls this only after an authoritative foreground
    /// snapshot and current-instance cursor have been read. UI values are not evidence.
    pub fn rebind(
        &self,
        binding: RuntimeBinding,
        baseline_sequence: u64,
    ) -> Result<(), ListenerError> {
        if self.bridge.binding().as_ref() != Some(&binding) {
            return Err(ListenerError::StaleBinding);
        }
        let mut current = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let epoch = current
            .as_ref()
            .map_or(Some(1), |state| state.epoch.checked_add(1))
            .ok_or(ListenerError::Unavailable)?;
        *current = Some(BufferState {
            binding,
            epoch,
            events: VecDeque::new(),
            latest: baseline_sequence,
            evicted_through: baseline_sequence,
            lost: false,
        });
        Ok(())
    }
    pub fn listen(&self, _caller: &HostCaller) -> Result<Listener, ListenerError> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let state = state.as_ref().ok_or(ListenerError::Unavailable)?;
        if self.bridge.binding().as_ref() != Some(&state.binding) {
            return Err(ListenerError::StaleBinding);
        }
        Ok(Listener {
            owner: self.state.clone(),
            binding: state.binding.clone(),
            epoch: state.epoch,
            active: AtomicBool::new(true),
        })
    }
    pub fn mark_gap(&self) {
        if let Some(state) = self
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            state.lost = true;
        }
    }
    pub fn publish(&self, event: &RawApiEvent) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let Some(state) = state.as_mut() else {
            return;
        };
        if self.bridge.binding().as_ref() != Some(&state.binding)
            || event.core_instance_id.as_deref()
                != Some(state.binding.runtime.core_instance_id.as_str())
            || event.config_revision != Some(state.binding.runtime.config_revision)
        {
            state.lost = true;
            return;
        }
        let Some(sequence) = event.sequence else {
            state.lost = true;
            return;
        };
        if sequence <= state.latest {
            return;
        } // duplicate/already covered by the baseline
        state.latest = sequence;
        if !zero_api::event_type::is_known(&event.event_type) {
            return;
        }
        if state.events.len() == CAPACITY {
            if let Some((sequence, _)) = state.events.pop_front() {
                state.evicted_through = sequence;
            }
        }
        state.events.push_back((
            sequence,
            EventHint {
                event_type: event.event_type.clone(),
                sequence: sequence.to_string(),
            },
        ));
    }
    pub fn replay(
        &self,
        listener: &Listener,
        after: u64,
        limit: usize,
    ) -> Result<HintReplay, ListenerError> {
        if !listener.is_active() {
            return Err(ListenerError::Cancelled);
        }
        if limit == 0 || limit > PAGE_LIMIT {
            return Err(ListenerError::InvalidBudget);
        }
        if !Arc::ptr_eq(&self.state, &listener.owner) {
            return Err(ListenerError::StaleBinding);
        }
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let state = state.as_ref().ok_or(ListenerError::Unavailable)?;
        if state.epoch != listener.epoch
            || state.binding != listener.binding
            || self.bridge.binding().as_ref() != Some(&listener.binding)
        {
            return Err(ListenerError::StaleBinding);
        }
        let has_gap = state.lost || after < state.evicted_through || after > state.latest;
        Ok(HintReplay {
            core_instance_id: state.binding.runtime.core_instance_id.clone(),
            config_revision: state.binding.runtime.config_revision.to_string(),
            profile_id: state.binding.profile_id.clone(),
            client_generation: state.binding.client_generation.to_string(),
            requested_after: after.to_string(),
            current_sequence: state.latest.to_string(),
            has_gap,
            events: if has_gap {
                Vec::new()
            } else {
                state
                    .events
                    .iter()
                    .filter(|(sequence, _)| *sequence > after)
                    .take(limit)
                    .map(|(_, hint)| hint.clone())
                    .collect()
            },
        })
    }
}
