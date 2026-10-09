//! One shared, short-lived OS snapshot; blocking callers coalesce on a mutex.
use crate::errors::AppResult;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub(super) struct StatusCache<T> {
    entry: Mutex<Option<(Instant, AppResult<T>)>>,
}
impl<T: Clone> StatusCache<T> {
    pub const fn new() -> Self {
        Self {
            entry: Mutex::new(None),
        }
    }
    pub fn get(&self, fresh: bool, fetch: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
        let mut entry = self.entry.lock().unwrap_or_else(|error| error.into_inner());
        if !fresh {
            if let Some((at, value)) = &*entry {
                let ttl = if value.is_ok() {
                    Duration::from_secs(5)
                } else {
                    Duration::from_secs(1)
                };
                if at.elapsed() < ttl {
                    return value.clone();
                }
            }
        }
        let value = fetch();
        *entry = Some((Instant::now(), value.clone()));
        value
    }
    pub fn invalidate(&self) {
        *self.entry.lock().unwrap_or_else(|error| error.into_inner()) = None;
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
