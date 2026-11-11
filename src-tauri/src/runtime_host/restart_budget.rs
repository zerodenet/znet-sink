use std::time::{Duration, Instant};

pub(super) struct RestartBudget {
    attempts: u32,
    limit: u32,
    stable_for: Duration,
    ready_since: Option<Instant>,
}

impl RestartBudget {
    pub(super) fn new(limit: u32, stable_for: Duration) -> Self {
        Self {
            attempts: 0,
            limit,
            stable_for,
            ready_since: None,
        }
    }
    pub(super) fn allow_next(&mut self, now: Instant) -> bool {
        if self
            .ready_since
            .take()
            .is_some_and(|since| now.saturating_duration_since(since) >= self.stable_for)
        {
            self.attempts = 0;
        }
        self.attempts = self.attempts.saturating_add(1);
        self.attempts <= self.limit
    }
    pub(super) fn attempt(&self) -> u32 {
        self.attempts
    }
    pub(super) fn mark_ready(&mut self, now: Instant) {
        self.ready_since = Some(now);
    }
}

#[cfg(test)]
#[path = "restart_budget_tests.rs"]
mod tests;
