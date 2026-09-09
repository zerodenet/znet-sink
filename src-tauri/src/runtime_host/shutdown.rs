use crate::{services::common::lock, state::app_state::AppState};
use tauri::{AppHandle, Manager};

/// Run while the event loop and async runtime are alive. The same cross-domain
/// guard as configuration changes covers capture cleanup and owned child exit.
pub async fn shutdown_managed_runtime(app_handle: AppHandle) {
    let state = app_handle.state::<AppState>();
    state
        .shutting_down_handle()
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let _operation = state.proxy_config_operation().lock().await;
    let stop_app = app_handle.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = stop_app.state::<AppState>();
        state.runtime_host().run("shutdown", false, || {
            state.next_core_process_monitor_generation();
            let owned = {
                let process = lock(state.runtime_host().process(), "core_process")?;
                process
                    .child
                    .as_ref()
                    .zip(process.endpoint.as_ref())
                    .map(|(child, endpoint)| (child.id(), endpoint.path.clone()))
            };
            if let Some((pid, endpoint)) = owned {
                let cleanup = tauri::async_runtime::block_on(async {
                    tokio::time::timeout(
                        std::time::Duration::from_secs(4),
                        crate::capture::shutdown::stop_owned_tun(pid, endpoint),
                    )
                    .await
                });
                let failure = match cleanup {
                    Ok(Ok(())) => None,
                    Ok(Err(error)) => Some(error.message),
                    Err(_) => Some("TUN cleanup timed out".into()),
                };
                if let Some(failure) = failure {
                    state.runtime_host().record_error(failure.clone());
                    crate::services::file_logger::line(&format!(
                        "shutdown: {failure}; continuing owned child cleanup"
                    ));
                }
            }
            super::stop::stop_with_proxy_restore(stop_app.clone(), state.clone(), true)
        })
    })
    .await;
    if let Err(error) = result
        .map_err(|e| e.to_string())
        .and_then(|r| r.map_err(|e| e.message))
    {
        state.runtime_host().record_error(error.clone());
        crate::services::file_logger::line(&format!(
            "shutdown: {error}; retained child will close its lifetime pipe on drop"
        ));
    }
}
