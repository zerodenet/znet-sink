use std::process::Child;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::errors::{AppError, AppResult};
use crate::kernel::{
    connection::{MultiplexedConnection, ScopedConnection},
    transport,
};
use crate::models::core::CoreEndpoint;

pub(super) fn wait_for_ready(child: &mut Child, endpoint: &CoreEndpoint) -> AppResult<()> {
    let pid = child.id();
    wait(
        || {
            child
                .try_wait()
                .map(|status| status.is_none())
                .map_err(|error| {
                    AppError::internal(format!("failed to inspect core startup: {error}"))
                })
        },
        |timeout| probe(endpoint, pid, timeout),
        Duration::from_secs(15),
        Duration::from_millis(100),
        Duration::from_millis(300),
    )
}

fn probe(endpoint: &CoreEndpoint, pid: u32, timeout: Duration) -> AppResult<()> {
    let deadline = Instant::now() + timeout;
    // Released Windows kernels may close a one-shot pipe before the response
    // is consumed. Use the acknowledged subscription handshake already used
    // by normal client control, and check both facts over that same peer.
    let scoped = ScopedConnection::connect(endpoint.clone(), timeout)?;
    let connection = scoped.connection();
    let health = query(connection, "health", deadline)?;
    if health.get("healthy").and_then(Value::as_bool) != Some(true) {
        return Err(AppError::internal("core IPC has not reported healthy"));
    }
    let runtime = query(connection, "runtime", deadline)?;
    if runtime.get("pid").and_then(Value::as_u64) != Some(u64::from(pid)) {
        return Err(AppError::internal(
            "core IPC belongs to a different process",
        ));
    }
    Ok(())
}

fn query(connection: &MultiplexedConnection, variant: &str, deadline: Instant) -> AppResult<Value> {
    let timeout = deadline.saturating_duration_since(Instant::now());
    if timeout.is_zero() {
        return Err(AppError::internal("core readiness probe timed out"));
    }
    let id = format!("znet-readiness-{variant}");
    let frame = transport::serialize_frame(
        &json!({"type": "query", "id": id, "request": {(variant): {}}}),
    )?;
    let response = tauri::async_runtime::block_on(connection.request(frame, id, timeout))?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(AppError::core_response(response));
    }
    response
        .get("result")
        .and_then(|result| result.get(variant))
        .cloned()
        .ok_or_else(|| AppError::internal(format!("core readiness response omitted {variant}")))
}

fn wait(
    mut alive: impl FnMut() -> AppResult<bool>,
    mut probe: impl FnMut(Duration) -> AppResult<()>,
    timeout: Duration,
    interval: Duration,
    stable_for: Duration,
) -> AppResult<()> {
    let deadline = Instant::now() + timeout;
    let mut healthy_since = None;
    let mut last_probe_error = None;
    loop {
        if !alive()? {
            return Err(AppError::internal(
                "core process exited before IPC became ready",
            ));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(readiness_timeout(last_probe_error));
        }
        match probe(remaining.min(Duration::from_millis(500))) {
            Ok(()) => {
                last_probe_error = None;
                if !alive()? {
                    return Err(AppError::internal(
                        "core process exited during IPC readiness check",
                    ));
                }
                // IPC starts before listener orchestration. Require sustained
                // responses so an immediate bind failure cannot look ready.
                let since = healthy_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= stable_for {
                    return Ok(());
                }
            }
            Err(error) if Instant::now() >= deadline => {
                return Err(readiness_timeout(Some(error)));
            }
            Err(error) => {
                healthy_since = None;
                last_probe_error = Some(error);
            }
        }
        std::thread::sleep(interval.min(deadline.saturating_duration_since(Instant::now())));
    }
}

fn readiness_timeout(cause: Option<AppError>) -> AppError {
    let mut error = AppError::internal(match &cause {
        Some(cause) => format!("core IPC readiness timed out: {}", cause.message),
        None => "core IPC readiness timed out".into(),
    });
    error.details = cause.map(|cause| json!({"lastProbeError": cause}));
    error
}

#[cfg(test)]
#[path = "readiness_tests.rs"]
mod tests;
