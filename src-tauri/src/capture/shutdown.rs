use crate::{
    errors::{AppError, AppResult},
    kernel::configuration::BoundControl,
    models::core::CoreIpcOptions,
};
use serde_json::json;

/// Called only with a child PID and endpoint captured from the runtime owner.
/// Queries and the single stop command cannot reconnect to a replacement peer.
pub(crate) async fn stop_owned_tun(pid: u32, endpoint: String) -> AppResult<()> {
    let control = BoundControl::connect(CoreIpcOptions {
        socket: Some(endpoint),
        timeout_ms: Some(3_000),
    })
    .await?;
    let runtime = control
        .call(json!({"type":"query","request":{"runtime":{}}}))
        .await?;
    let runtime = runtime.get("runtime").unwrap_or(&runtime);
    if runtime.get("pid").and_then(serde_json::Value::as_u64) != Some(u64::from(pid)) {
        return Err(AppError::internal(
            "TUN cleanup skipped: endpoint is not the owned child",
        ));
    }
    let before = control.tun_status().await?;
    if !before.enabled {
        return stopped_without_error(before);
    }
    control
        .call(json!({"type":"command","method":"tun.stop","params":{}}))
        .await?;
    stopped_without_error(control.tun_status().await?)
}

fn stopped_without_error(status: crate::models::zero_runtime::GuiTunStatus) -> AppResult<()> {
    if status.enabled {
        return Err(AppError::internal("TUN cleanup was not confirmed"));
    }
    if let Some(error) = status.last_error {
        return Err(AppError::internal(format!("TUN cleanup: {error}")));
    }
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "shutdown_tests.rs"]
mod tests;
