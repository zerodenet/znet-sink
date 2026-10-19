use std::sync::atomic::{AtomicU8, Ordering};
use tauri::Manager;

/// Seal guest entry points synchronously before either exit path starts runtime
/// teardown. Waiting for in-flight work never claims to withdraw native calls.
pub(super) fn prepare(app: &tauri::AppHandle) -> znet_plugin_sandbox::runtime::Executor {
    #[cfg(target_os = "macos")]
    crate::services::macos_privilege::begin_shutdown();
    let state = app.state::<crate::state::app_state::AppState>();
    state.shutting_down_handle().store(true, Ordering::SeqCst);
    state.plugins().close_page_documents();
    let executor = state.plugins().executor_shutdown_handle();
    executor.close(std::time::Duration::ZERO);
    executor
}

pub(super) async fn drain_plugins(executor: znet_plugin_sandbox::runtime::Executor) {
    let remaining = tauri::async_runtime::spawn_blocking(move || {
        executor.close(std::time::Duration::from_secs(2))
    })
    .await;
    match remaining {
        Ok(state) if !state.drained => crate::services::file_logger::line(&format!(
            "plugin execution still draining: {} active; native calls were not withdrawn",
            state.running
        )),
        Err(_) => {
            crate::services::file_logger::line("plugin execution shutdown result unconfirmed")
        }
        _ => {}
    }
}

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
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
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
