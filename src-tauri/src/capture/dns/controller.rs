use super::preflight::{effective_dns, explicit_upstreams, probe};
use super::*;

async fn reconcile(
    state: &crate::state::app_state::AppState,
    tun: GuiTunStatus,
    pid: u32,
    options: crate::models::core::CoreIpcOptions,
) -> AppResult<()> {
    let candidate = target(&tun, pid);
    let Some(next) = (match candidate {
        Ok(next) => next,
        Err(error) => {
            let message = error.message.clone();
            tokio::task::spawn_blocking(move || record_failure(&tun, pid, message))
                .await
                .map_err(|e| AppError::internal(e.to_string()))??;
            return Err(error);
        }
    }) else {
        return tokio::task::spawn_blocking(release)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
    };
    let validation = (|| {
        let (app, source) = crate::configuration::preferences::current(state)?;
        let dns = effective_dns(&app, &source)?;
        explicit_upstreams(&dns, &next.server)?;
        Ok::<_, AppError>(())
    })();
    if let Err(error) = validation {
        let message = error.message.clone();
        tokio::task::spawn_blocking(move || record_failure(&tun, pid, message))
            .await
            .map_err(|e| AppError::internal(e.to_string()))??;
        return Err(error);
    }
    let existing = STATE
        .lock()
        .map_err(|_| AppError::internal("TUN DNS state lock poisoned"))?
        .target
        .as_ref()
        == Some(&next);
    if !existing {
        let result = async {
            let control =
                crate::kernel::configuration::BoundControl::connect(options.clone()).await?;
            let identity = control
                .call(serde_json::json!({"type":"query","request":{"runtime":{}}}))
                .await?;
            let identity = identity.get("runtime").unwrap_or(&identity);
            if identity.get("pid").and_then(serde_json::Value::as_u64) != Some(u64::from(pid)) {
                return Err(AppError::internal("DNS 接管拒绝操作非当前客户端管理的内核"));
            }
            let peer = next.server.clone();
            tokio::task::spawn_blocking(move || probe(&peer))
                .await
                .map_err(|e| AppError::internal(e.to_string()))??;
            let after = control.tun_status().await?;
            if after.name != tun.name
                || !after.enabled
                || !after.healthy
                || after.dns_hijacked_queries <= tun.dns_hijacked_queries
            {
                return Err(AppError::internal(
                    "DNS 探测未被当前 TUN 内核确认拦截，未修改系统 DNS",
                ));
            }
            Ok(())
        }
        .await;
        if let Err(error) = result {
            let message = error.message.clone();
            tokio::task::spawn_blocking(move || {
                let mut current = STATE
                    .lock()
                    .map_err(|_| AppError::internal("TUN DNS state lock poisoned"))?;
                stop(&mut current)?;
                current.target = Some(next);
                current.error = Some(message);
                Ok::<_, AppError>(())
            })
            .await
            .map_err(|e| AppError::internal(e.to_string()))??;
            return Err(error);
        }
    }
    tokio::task::spawn_blocking(move || ensure(&tun, pid))
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
}

/// Reconcile profile-owned and command-owned TUN with the same operation lock.
/// Only the GUI's current child can authorize a DNS resolver installation.
pub(crate) fn spawn(app: tauri::AppHandle) {
    use tauri::Manager;
    tauri::async_runtime::spawn(async move {
        let mut last_error = None;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let state = app.state::<crate::state::app_state::AppState>();
            if state.is_shutting_down() {
                break;
            }
            let Ok(_operation) = state.proxy_config_operation().try_lock() else {
                continue;
            };
            let owned = state.runtime_host().owned_endpoint();
            OWNED.store(owned.is_some(), std::sync::atomic::Ordering::Relaxed);
            let result = if let Some((pid, socket)) = owned {
                match crate::kernel::zero::runtime::tun_status(Some(
                    crate::models::core::CoreIpcOptions {
                        socket: Some(socket.clone()),
                        timeout_ms: Some(2000),
                    },
                ))
                .await
                {
                    Ok(tun) => {
                        reconcile(
                            state.inner(),
                            tun,
                            pid,
                            crate::models::core::CoreIpcOptions {
                                socket: Some(socket),
                                timeout_ms: Some(2000),
                            },
                        )
                        .await
                    }
                    // An IPC timeout is unknown, not permission to rewrite DNS.
                    Err(error) => Err(error),
                }
            } else {
                tokio::task::spawn_blocking(release)
                    .await
                    .unwrap_or_else(|e| Err(AppError::internal(e.to_string())))
            };
            let message = result.err().map(|e| e.message);
            if message != last_error {
                if let Some(message) = &message {
                    crate::services::file_logger::line(&format!("TUN DNS: {message}"));
                }
                last_error = message;
            }
        }
    });
}
