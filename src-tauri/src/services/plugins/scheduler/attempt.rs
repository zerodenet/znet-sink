//! A bounded execution journal, never a durable authorization or replay token.
use super::*;
use std::sync::{atomic::AtomicBool, Weak};

#[derive(Default)]
pub(in crate::services::plugins) struct Owner {
    active: Mutex<BTreeSet<String>>,
    started: AtomicBool,
    closed: AtomicBool,
    pub(super) wake: tokio::sync::Notify,
}

pub(super) struct Flight {
    owner: Weak<Owner>,
    id: String,
}
impl Drop for Flight {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.upgrade() {
            if let Ok(mut active) = owner.active.lock() {
                active.remove(&self.id);
            }
        }
    }
}
impl Owner {
    pub(in crate::services::plugins) fn notify(&self) {
        self.wake.notify_one();
    }
    pub(super) fn start(&self) -> bool {
        self.notify();
        if self.is_closed() {
            return false;
        }
        !self.started.swap(true, std::sync::atomic::Ordering::AcqRel)
    }
    pub(super) fn is_closed(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::Acquire)
    }
    #[cfg(any(target_os = "android", target_os = "ios", test))]
    pub(in crate::services::plugins) fn close(&self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::Release);
        self.notify();
    }
    pub(super) fn hold(self: &Arc<Self>, id: &str) -> AppResult<Arc<Flight>> {
        if !self.active.lock().map_err(|_| stale())?.insert(id.into()) {
            return Err(stale());
        }
        Ok(Arc::new(Flight {
            owner: Arc::downgrade(self),
            id: id.into(),
        }))
    }
    fn contains(&self, id: &str) -> bool {
        self.active.lock().is_ok_and(|active| active.contains(id))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::services::plugins) enum Phase {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
    Unconfirmed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in crate::services::plugins) struct Attempt {
    pub(super) id: String,
    pub(super) state: Phase,
    // A digest identifies the original review without persisting any lease.
    binding: String,
    pub(super) queued_at_unix_ms: String,
    pub(super) dispatch_at_unix_ms: Option<String>,
    pub(super) completed_at_unix_ms: Option<String>,
    pub(super) observed_at_unix_ms: String,
}

pub(super) fn random_id() -> AppResult<String> {
    let mut nonce = [0u8; 24];
    getrandom::fill(&mut nonce).map_err(|_| AppError::internal("无法创建插件任务编号"))?;
    Ok(nonce.iter().map(|byte| format!("{byte:02x}")).collect())
}
fn hex(value: &str, size: usize) -> bool {
    value.len() == size
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn binding(review: &Review) -> AppResult<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(review).map_err(io::failure)?)
    ))
}
impl Attempt {
    pub(super) fn queued(now: u64, review: &Review) -> AppResult<Self> {
        Ok(Self {
            id: random_id()?,
            state: Phase::Queued,
            binding: binding(review)?,
            queued_at_unix_ms: now.to_string(),
            dispatch_at_unix_ms: None,
            completed_at_unix_ms: None,
            observed_at_unix_ms: now.to_string(),
        })
    }
    pub(super) fn valid(&self) -> bool {
        let timestamp = |value: &str| value.parse::<u64>().is_ok_and(|n| n.to_string() == value);
        hex(&self.id, 48)
            && hex(&self.binding, 64)
            && timestamp(&self.queued_at_unix_ms)
            && timestamp(&self.observed_at_unix_ms)
            && self.dispatch_at_unix_ms.as_deref().is_none_or(timestamp)
            && self.completed_at_unix_ms.as_deref().is_none_or(timestamp)
            && match self.state {
                Phase::Queued | Phase::Cancelled => {
                    self.dispatch_at_unix_ms.is_none() && self.completed_at_unix_ms.is_none()
                }
                Phase::Running | Phase::Unconfirmed => {
                    self.dispatch_at_unix_ms.is_some() && self.completed_at_unix_ms.is_none()
                }
                Phase::Completed | Phase::Failed => {
                    self.dispatch_at_unix_ms.is_some() && self.completed_at_unix_ms.is_some()
                }
            }
    }
    pub(super) fn matches(&self, due: &DueTask) -> AppResult<bool> {
        Ok(due
            .task
            .last_attempt
            .as_ref()
            .is_some_and(|a| a.id == self.id)
            && self.binding == binding(&due.review)?
            && matches!(self.state, Phase::Queued | Phase::Running))
    }
    pub(super) fn recover(
        &mut self,
        owner: &Owner,
        review: Option<&Review>,
        now: u64,
    ) -> AppResult<bool> {
        if !matches!(self.state, Phase::Queued | Phase::Running) {
            return Ok(false);
        }
        let current = review
            .map(binding)
            .transpose()?
            .is_some_and(|b| b == self.binding);
        if current && owner.contains(&self.id) {
            return Ok(false);
        }
        self.state = match self.state {
            Phase::Queued => Phase::Cancelled,
            _ => Phase::Unconfirmed,
        };
        self.observed_at_unix_ms = now.to_string();
        Ok(true)
    }
    pub(super) fn blocks_dispatch(&self) -> bool {
        matches!(
            self.state,
            Phase::Queued | Phase::Running | Phase::Unconfirmed
        )
    }
}

#[derive(Clone, Copy)]
pub(super) enum Completion {
    Known(bool),
    Unconfirmed,
}
