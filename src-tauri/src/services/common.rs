use std::collections::HashSet;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::errors::{AppError, AppResult};

/// Create a `Command` that never spawns a visible console window on Windows.
/// All external process invocations must use this instead of `Command::new`.
pub(crate) fn background_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x08000000);
        // CREATE_NO_WINDOW
    }
    cmd
}

static STORE_ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn lock<'a, T>(
    mutex: &'a Mutex<T>,
    label: &'static str,
) -> AppResult<MutexGuard<'a, T>> {
    mutex
        .lock()
        .map_err(|_| AppError::internal(format!("{label} state lock is poisoned")))
}

#[derive(Debug)]
pub(crate) struct InFlightGuard<'a> {
    registry: &'a Mutex<HashSet<String>>,
    id: String,
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut registry) = self.registry.lock() {
            registry.remove(&self.id);
        }
    }
}

pub(crate) fn begin_in_flight<'a>(
    registry: &'a Mutex<HashSet<String>>,
    resource: &'static str,
    id: &str,
) -> AppResult<InFlightGuard<'a>> {
    let mut active = lock(registry, "in_flight")?;
    if !active.insert(id.to_string()) {
        return Err(AppError::conflict(
            resource,
            id,
            format!("{resource} operation is already in progress"),
        ));
    }
    Ok(InFlightGuard {
        registry,
        id: id.to_string(),
    })
}

pub(crate) fn is_in_flight(registry: &Mutex<HashSet<String>>, id: &str) -> AppResult<bool> {
    Ok(lock(registry, "in_flight")?.contains(id))
}

pub(crate) fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

pub(crate) fn normalize_required(value: String, field: &'static str) -> AppResult<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(AppError::invalid_argument(format!(
            "{field} must not be empty"
        )));
    }
    Ok(value)
}

pub(crate) fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    })
}

pub(crate) fn generated_store_id(prefix: &str) -> String {
    let now = now_unix_ms();
    let sequence = STORE_ID_SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1;
    format!("{prefix}_{now:x}{:x}{sequence:x}", std::process::id())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Mutex;

    use super::{begin_in_flight, generated_store_id};

    #[test]
    fn generated_store_id_uses_prefix_without_order_semantics() {
        let first = generated_store_id("proxy-config");
        let second = generated_store_id("proxy-config");

        assert!(first.starts_with("proxy-config_"));
        assert!(second.starts_with("proxy-config_"));
        assert_ne!(first, second);
        assert!(!first.starts_with("proxy-config-"));
    }

    #[test]
    fn in_flight_guard_rejects_duplicate_and_releases_on_drop() {
        let registry = Mutex::new(HashSet::new());
        let first = begin_in_flight(&registry, "subscription", "sub-1").unwrap();
        let duplicate = begin_in_flight(&registry, "subscription", "sub-1").unwrap_err();
        assert_eq!(duplicate.code, "conflict");
        drop(first);
        assert!(begin_in_flight(&registry, "subscription", "sub-1").is_ok());
    }
}
