use std::process::Child;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::errors::{AppError, AppResult};
use crate::kernel::transport;
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
    let started = Instant::now();
    let health = query(endpoint, "health", timeout)?;
    if health.get("healthy").and_then(Value::as_bool) != Some(true) {
        return Err(AppError::internal("core IPC has not reported healthy"));
    }
    let remaining = timeout.saturating_sub(started.elapsed());
    if remaining.is_zero() {
        return Err(AppError::internal("core readiness probe timed out"));
    }
    let runtime = query(endpoint, "runtime", remaining)?;
    if runtime.get("pid").and_then(Value::as_u64) != Some(u64::from(pid)) {
        return Err(AppError::internal(
            "core IPC belongs to a different process",
        ));
    }
    Ok(())
}

fn query(endpoint: &CoreEndpoint, variant: &str, timeout: Duration) -> AppResult<Value> {
    let frame = transport::serialize_frame(&json!({"type": "query", "request": {(variant): {}}}))?;
    let response = transport::send_json_line_request(endpoint.clone(), frame, timeout)?;
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
    loop {
        if !alive()? {
            return Err(AppError::internal(
                "core process exited before IPC became ready",
            ));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(AppError::internal("core IPC readiness timed out"));
        }
        match probe(remaining.min(Duration::from_millis(500))) {
            Ok(()) => {
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
                return Err(AppError::internal(format!(
                    "core IPC readiness timed out: {}",
                    error.message
                )));
            }
            Err(_) => healthy_since = None,
        }
        std::thread::sleep(interval.min(deadline.saturating_duration_since(Instant::now())));
    }
}

#[cfg(test)]
#[path = "readiness_tests.rs"]
mod tests;
