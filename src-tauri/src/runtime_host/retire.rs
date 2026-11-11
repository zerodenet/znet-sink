//! One retirement path for requested stops, watchdog recovery and failed starts.
use super::child::ManagedChild as Child;
use crate::errors::{AppError, AppResult};
use std::{
    io,
    process::ExitStatus,
    time::{Duration, Instant},
};

pub(super) const GRACEFUL_EXIT_TIMEOUT: Duration = Duration::from_secs(5);
// tun.stop can wait for both the packet runtime and route cleanup. Its normal
// command response budget is 60 seconds; do not cancel it after four seconds.
const TUN_CLEANUP_TIMEOUT: Duration = Duration::from_secs(75);
// Also allow the bounded native process/job wait and interface retirement audit.
pub(crate) const EXIT_CLEANUP_WAIT: Duration = Duration::from_secs(105);

pub(super) struct Retirement {
    pub exit: io::Result<ExitStatus>,
    pub tun_cleanup: AppResult<()>,
}

pub(super) fn retire_owned_child(child: &mut Child, endpoint: Option<&str>) -> Retirement {
    retire_with_cleanup(child, GRACEFUL_EXIT_TIMEOUT, |pid| {
        let endpoint = endpoint.ok_or_else(|| {
            AppError::internal("TUN cleanup cannot be confirmed without the owned control endpoint")
        })?;
        tauri::async_runtime::block_on(async {
            tokio::time::timeout(
                TUN_CLEANUP_TIMEOUT,
                crate::capture::shutdown::stop_owned_tun(pid, endpoint.to_owned()),
            )
            .await
            .map_err(|_| AppError::internal("TUN cleanup timed out"))?
        })
    })
}

fn retire_with_cleanup(
    child: &mut Child,
    graceful_timeout: Duration,
    cleanup: impl FnOnce(u32) -> AppResult<()>,
) -> Retirement {
    retire_with(child, cleanup, |child| {
        stop_owned_child(child, graceful_timeout)
    })
}

fn retire_with(
    child: &mut Child,
    cleanup: impl FnOnce(u32) -> AppResult<()>,
    wait: impl FnOnce(&mut Child) -> io::Result<ExitStatus>,
) -> Retirement {
    match child.try_wait() {
        // A crashed child has no IPC peer left to stop. Its actual exit must
        // still be consumed before the caller releases the slot.
        Ok(Some(status)) => Retirement {
            exit: Ok(status),
            tun_cleanup: Err(AppError::internal(
                "old kernel exited before TUN cleanup could be confirmed",
            )),
        },
        Err(error) => Retirement {
            exit: Err(error),
            tun_cleanup: Err(AppError::internal("old kernel exit could not be inspected")),
        },
        Ok(None) => {
            let tun_cleanup = cleanup(child.id());
            // A failed/lost TUN acknowledgement must not strand the child.
            // The caller retains it on any wait/termination error and may not
            // replace it. A TUN error also blocks this restart after reaping.
            let exit = wait(child);
            Retirement { exit, tun_cleanup }
        }
    }
}

pub(super) fn stop_owned_child(
    child: &mut Child,
    graceful_timeout: Duration,
) -> io::Result<ExitStatus> {
    child.stdin.take();
    let deadline = Instant::now() + graceful_timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            if let Err(error) = child.kill() {
                // The child can exit between the last poll and termination.
                return child.try_wait()?.ok_or(error);
            }
            return child.wait();
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(test)]
#[path = "retire_tests.rs"]
mod tests;
