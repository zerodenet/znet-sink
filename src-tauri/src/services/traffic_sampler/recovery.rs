use std::time::{Duration, Instant};

const PUSH_TIMEOUT: Duration = Duration::from_secs(3);
const RETRY_INTERVAL: Duration = Duration::from_secs(1);

/// Only missing fresh pushes trigger reads. Query success must not stop
/// recovery polling while the push stream itself remains silent.
pub(crate) struct Recovery {
    last_push: Instant,
    last_attempt: Option<Instant>,
}
impl Recovery {
    pub fn new(now: Instant) -> Self {
        Self {
            last_push: now,
            last_attempt: None,
        }
    }
    pub fn pushed(&mut self, now: Instant, sampled_at: u64, received_at: u64) {
        if received_at.saturating_sub(sampled_at) <= PUSH_TIMEOUT.as_millis() as u64 {
            self.last_push = now;
            self.last_attempt = None;
        }
    }
    pub fn due(&mut self, now: Instant) -> bool {
        if now.duration_since(self.last_push) < PUSH_TIMEOUT
            || self
                .last_attempt
                .is_some_and(|at| now.duration_since(at) < RETRY_INTERVAL)
        {
            return false;
        }
        self.last_attempt = Some(now);
        true
    }
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
