use super::preflight::{explicit_upstreams, probe};
use super::*;

const VERIFY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

struct Verified {
    target: Target,
    identity: znet_engine_client::configuration::RuntimeIdentity,
    checked: std::time::Instant,
}

async fn reconcile(
    state: &crate::state::app_state::AppState,
    verified: &mut Option<Verified>,
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
    let existing = STATE
        .lock()
        .map_err(|_| AppError::internal("TUN DNS state lock poisoned"))?
        .target
        .as_ref()
        == Some(&next);
    // IPC failure remains Unknown: it cannot authorize withdrawing or replacing
    // a working resolver. Only a confirmed identity/config/packet failure can.
    let control = crate::kernel::configuration::BoundControl::connect(options).await?;
    let identity = control
        .call(serde_json::json!({"type":"query","request":{"runtime":{}}}))
        .await?;
    let identity = identity.get("runtime").unwrap_or(&identity);
    if identity.get("pid").and_then(serde_json::Value::as_u64) != Some(u64::from(pid)) {
        return fail(
            &tun,
            pid,
            AppError::internal("DNS 接管拒绝操作非当前客户端管理的内核"),
        )
        .await;
    }
    let identity = crate::kernel::zero::queries::parse_runtime_identity(identity)?;
    let verify = !existing
        || verified.as_ref().is_none_or(|verified| {
            verified.target != next
                || verified.identity != identity
                || verified.checked.elapsed() >= VERIFY_INTERVAL
        });
    if !verify {
        return tokio::task::spawn_blocking(move || ensure(&tun, pid))
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
    }
    let validation = (|| {
        let config = crate::configuration::preferences::composed_current(state)?;
        let dns = config
            .pointer("/runtime/dns")
            .ok_or_else(|| AppError::internal("最终内核配置未启用独立 DNS，未接管系统 DNS"))?;
        explicit_upstreams(dns, &next.server)?;
        Ok::<_, AppError>(())
    })();
    if let Err(error) = validation {
        let message = error.message.clone();
        tokio::task::spawn_blocking(move || record_failure(&tun, pid, message))
            .await
            .map_err(|e| AppError::internal(e.to_string()))??;
        return Err(error);
    }
    // Probe the route and the actual packet interceptor before publication.
    verify_packets(&control, &tun, &next, pid).await?;
    let install_tun = tun.clone();
    tokio::task::spawn_blocking(move || ensure(&install_tun, pid))
        .await
        .map_err(|e| AppError::internal(e.to_string()))??;
    // Publication itself can trigger OS DNS discovery and route reconciliation.
    // Recheck now and periodically thereafter, including after link recovery.
    let after = control.tun_status().await?;
    verify_packets(&control, &after, &next, pid).await?;
    *verified = Some(Verified {
        target: next,
        identity,
        checked: std::time::Instant::now(),
    });
    Ok(())
}

async fn verify_packets(
    control: &crate::kernel::configuration::BoundControl,
    before: &GuiTunStatus,
    target: &Target,
    pid: u32,
) -> AppResult<()> {
    let peer = target.server.clone();
    let packet = async move {
        tokio::task::spawn_blocking(move || probe(&peer))
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
    };
    match verification::verify(before, packet, || control.tun_status()).await {
        Ok(()) => Ok(()),
        Err(verification::Failure::Unknown(error)) => Err(error),
        Err(verification::Failure::Confirmed(error)) => fail(before, pid, error).await,
    }
}

async fn fail(tun: &GuiTunStatus, pid: u32, error: AppError) -> AppResult<()> {
    let tun = tun.clone();
    let message = error.message.clone();
    tokio::task::spawn_blocking(move || record_failure(&tun, pid, message))
        .await
        .map_err(|e| AppError::internal(e.to_string()))??;
    Err(error)
}

/// Reconcile profile-owned and command-owned TUN with the same operation lock.
/// Only the GUI's current child can authorize a DNS resolver installation.
pub(crate) fn spawn(app: tauri::AppHandle) {
    use tauri::Manager;
    tauri::async_runtime::spawn(async move {
        let mut last_error = None;
        let mut verified = None;
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
                            &mut verified,
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
