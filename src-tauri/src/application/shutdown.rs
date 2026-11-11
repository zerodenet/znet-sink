use std::sync::atomic::{AtomicU8, Ordering};

/// macOS native termination can deliver Exit without ExitRequested. Finish
/// cleanup inside that callback, before Cocoa terminates the process. The final
/// Exit after our asynchronous requested-exit cleanup must not replay it.
pub(super) fn complete_final_exit(
    event: &tauri::RunEvent,
    stage: &AtomicU8,
    persist_intent: impl FnOnce(),
    cleanup: impl FnOnce(),
) {
    if !matches!(event, tauri::RunEvent::Exit) || stage.load(Ordering::SeqCst) == 2 {
        return;
    }
    // Native quit can interrupt requested-exit cleanup before it obtains the
    // mutation lock. Persist OFF before Cocoa exits even in that case.
    persist_intent();
    match stage.compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst) {
        Ok(_) => {
            cleanup();
            stage.store(2, Ordering::SeqCst);
        }
        Err(1) => {
            // Drain the existing cleanup instead of replaying network commands.
            // A native exit must not overtake the already-running TUN stop
            // (60s response budget plus queries and child-exit confirmation).
            let deadline = std::time::Instant::now() + crate::runtime_host::EXIT_CLEANUP_WAIT;
            while stage.load(Ordering::SeqCst) == 1 && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            if stage.load(Ordering::SeqCst) != 2 {
                crate::services::file_logger::line(
                    "shutdown: native Exit timed out waiting for requested-exit cleanup",
                );
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "shutdown_tests.rs"]
mod tests;
