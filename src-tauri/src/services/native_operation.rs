use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};

use znet_client_core::capability::Permission;

use crate::{
    errors::{AppError, AppResult},
    state::app_state::AppState,
};

pub async fn execute_async<T>(
    state: &AppState,
    capability: &str,
    scope: &str,
    timeout: Duration,
    work: impl Future<Output = AppResult<T>>,
) -> AppResult<T> {
    let permission = Permission::new(capability, scope);
    let lease = znet_client_capabilities::host::operation(
        state.capabilities(),
        &format!("builtin.operation:{capability}"),
        permission.clone(),
        1,
        1,
        timeout,
    )
    .map_err(capability_error)?;
    let failure = Arc::new(Mutex::new(None));
    let reported = Arc::clone(&failure);
    let resource = lease
        .execute_async(&permission, async move {
            work.await.map(|value| (value, 0)).map_err(|error| {
                *reported
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(error);
                znet_client_core::capability::Error::Transport
            })
        })
        .await
        .and_then(|resource| resource.take(&lease));
    resource.map_err(|error| {
        failure
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .unwrap_or_else(|| capability_error(error))
    })
}

fn capability_error(error: znet_client_core::capability::Error) -> AppError {
    AppError::internal(format!("客户端操作被能力策略拒绝：{error}"))
}
