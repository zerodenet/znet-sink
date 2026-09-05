use std::future::Future;
use std::time::Duration;

use crate::errors::{AppError, AppResult};
use crate::models::zero_runtime::GuiTunStatus;

pub(super) async fn restart_intent<Q, F>(
    saved: Option<bool>,
    running: bool,
    query: Q,
) -> AppResult<bool>
where
    Q: FnOnce() -> F,
    F: Future<Output = AppResult<bool>>,
{
    match saved {
        Some(value) => Ok(value),
        None if running => query().await,
        None => Ok(false),
    }
}

// Submit at most one mutation. A lost reply is reconciled through status reads,
// never by starting another overlapping platform operation.
pub(super) async fn restore<Q, QF, E, EF>(
    mut query: Q,
    mut enable: E,
    timeout: Duration,
    interval: Duration,
) -> AppResult<()>
where
    Q: FnMut() -> QF,
    QF: Future<Output = AppResult<GuiTunStatus>>,
    E: FnMut() -> EF,
    EF: Future<Output = AppResult<GuiTunStatus>>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    let mut submitted = false;
    loop {
        let result = query().await;
        let error = match result {
            Ok(status) if status.enabled && status.healthy => return Ok(()),
            Ok(status) if !status.supported => {
                return Err(AppError::invalid_argument(
                    "the current Zero runtime does not support TUN",
                ));
            }
            Ok(status) if !status.enabled && !submitted => {
                submitted = true;
                match enable().await {
                    Ok(status) if status.enabled && status.healthy => return Ok(()),
                    Err(error) if !transient(&error) => return Err(error),
                    Err(error) => Some(error),
                    Ok(_) => None,
                }
            }
            Ok(_) => None,
            Err(error) if !transient(&error) => return Err(error),
            Err(error) => Some(error),
        };
        if tokio::time::Instant::now() >= deadline {
            return Err(error.unwrap_or_else(|| AppError {
                code: "tun_restore_unconfirmed",
                message: "Core restarted, but TUN did not confirm that it is running".into(),
                details: None,
            }));
        }
        tokio::time::sleep(interval).await;
    }
}

fn transient(error: &AppError) -> bool {
    matches!(
        error.code,
        "timeout" | "connection_closed" | "core_unavailable" | "io_error"
    )
}

#[cfg(test)]
#[path = "tests/tun_restore.rs"]
mod tests;
