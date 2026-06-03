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
        std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x08000000); // CREATE_NO_WINDOW
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
    use super::generated_store_id;

    #[test]
    fn generated_store_id_uses_prefix_without_order_semantics() {
        let first = generated_store_id("proxy-config");
        let second = generated_store_id("proxy-config");

        assert!(first.starts_with("proxy-config_"));
        assert!(second.starts_with("proxy-config_"));
        assert_ne!(first, second);
        assert!(!first.starts_with("proxy-config-"));
    }
}
