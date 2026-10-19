use crate::{services::common::lock, state::app_state::AppState};
use tauri::{AppHandle, Manager};

pub(crate) fn persist_exit_tun_intent(app_handle: &AppHandle) {
    let state = app_handle.state::<AppState>();
    let saved_intent = (|| {
        let mut config = lock(state.app_config(), "app_config")?;
        crate::capture::shutdown::clear_saved_tun_intent(
            &mut config,
            &crate::services::app_config_store::default_config_path()?,
        )
    })();
    if let Err(error) = saved_intent {
        state.runtime_host().record_error(error.message.clone());
        crate::services::file_logger::line(&format!(
            "shutdown: failed to save TUN OFF; next launch may restore TUN: {}",
            error.message
        ));
    }
}

/// Run while the event loop and async runtime are alive. The same cross-domain
/// guard as configuration changes covers capture cleanup and owned child exit.
pub async fn shutdown_managed_runtime(app_handle: AppHandle) {
    #[cfg(target_os = "macos")]
    crate::services::macos_privilege::begin_shutdown();
    let state = app_handle.state::<AppState>();
    state
        .shutting_down_handle()
        .store(true, std::sync::atomic::Ordering::SeqCst);
    #[cfg(feature = "tool-node-probe")]
    {
        state.probe_runtime().invalidate(
            crate::client_core::ProbeJobState::InvalidatedByCoreRestart,
            crate::services::common::now_unix_ms(),
        );
        state.checkpoint_probe_jobs();
    }
    #[cfg(any(feature = "tool-dns", feature = "tool-route"))]
    {
        state
            .tool_runtime()
            .interrupt_for_shutdown(crate::services::common::now_unix_ms());
        if let Err(error) = state.tool_runtime().checkpoint(state.capabilities()) {
            crate::services::file_logger::line(&format!(
                "shutdown: failed to persist interrupted tool jobs: {}",
                error.message
            ));
        }
    }
    state.capabilities().shutdown();
    let _operation = state.proxy_config_operation().lock().await;
    let stop_app = app_handle.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = stop_app.state::<AppState>();
        // Persist OFF even if the core already exited or TUN cleanup fails.
        // A storage failure must not prevent stopping the owned runtime.
        persist_exit_tun_intent(&stop_app);
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
            let cleanup_proxy = lock(state.app_config(), "app_config")
                .map(|config| config.core.cleanup_proxy_on_exit)
                .unwrap_or(true);
            super::stop::stop_for_exit(stop_app.clone(), state.clone(), cleanup_proxy)
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
