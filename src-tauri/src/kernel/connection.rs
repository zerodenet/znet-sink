//! Multiplexed kernel connection — one long-lived connection carrying both
//! request/response traffic and kernel event pushes.
//!
//! ## Why one connection
//!
//! The kernel keeps a connection open only if its first frame is a
//! `subscribe`; non-subscribe connections are closed after a single
//! response. So we open exactly one connection, send `subscribe` first, and
//! reuse that same connection for every subsequent `query` / `command` /
//! `ping` (each with a unique `id`).
//!
//! ## Frame routing
//!
//! A background reader thread classifies every incoming frame:
//!
//! | frame shape | classification | action |
//! |-------------|----------------|--------|
//! | top-level `ok` present | response | pair by `id` with the waiting [`oneshot`] |
//! | no top-level `ok` | event | broadcast to all event subscribers |
//! | line starting with `:` | heartbeat | ignored (handled in [`transport::read_json_line`]) |
//!
//! Response frames without a matching waiter (e.g. the initial `subscribe`
//! ack) are dropped silently.
//!
//! ## Lifecycle
//!
//! At most one live connection per endpoint path, held by a global manager
//! ([`get_or_connect`]). When the kernel stops or the pipe breaks, the reader
//! drains every pending waiter with a `connection_closed` error and marks the
//! connection dead; the next [`get_or_connect`] rebuilds it.

use std::collections::HashMap;
use std::io::{BufReader, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, LazyLock, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::{broadcast, oneshot};

/// IPC is an external failure boundary. If a worker panics while holding one
/// of these coordination locks, retain the protected state and let later
/// calls reconnect instead of cascading the poisoned lock into more panics.
fn recover_lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
use tokio::time::timeout;

use crate::errors::{AppError, AppResult};
use crate::kernel::transport::{self, KernelCloser, KernelReader, KernelWriter};
use crate::models::core::CoreEndpoint;
use crate::models::debug::{push_debug_frame, DebugFrame};

/// `id` used for the initial `subscribe` frame. Distinct from the
/// `znet-sink-<n>` request ids so it can never collide with a pending
/// request, and its ack is simply dropped by the reader.
const SUBSCRIBE_FRAME_ID: &str = "znet-sink-subscribe";

/// Broadcast channel capacity for kernel events.
///
/// Generous on purpose: traffic stats and flow events can burst. A slow
/// consumer falls behind and [`broadcast::Receiver`] reports
/// [`Lagged`](broadcast::error::RecvError::Lagged), which the event
/// forwarders tolerate.
const EVENT_CHANNEL_CAPACITY: usize = 1024;

/// A shared multiplexed connection. Cheap to clone (internally `Arc`).
#[derive(Clone)]
pub struct MultiplexedConnection {
    inner: Arc<Inner>,
}

struct Inner {
    endpoint: CoreEndpoint,
    writer: Mutex<KernelWriter>,
    closer: KernelCloser,
    pending: Mutex<HashMap<String, oneshot::Sender<Result<Value, AppError>>>>,
    event_tx: broadcast::Sender<Value>,
    alive: AtomicBool,
    last_received_at_ms: AtomicU64,
    /// Set during `connect()` so the reader can signal when the subscribe
    /// acknowledgement arrives.  Taken by the reader on first response
    /// frame whose `id` matches [`SUBSCRIBE_FRAME_ID`], then read by
    /// `connect()` before it returns.
    subscribe_ack_tx: Mutex<Option<mpsc::SyncSender<AppResult<()>>>>,
}

impl MultiplexedConnection {
    /// Open a connection, send the initial `subscribe` frame, and spawn the
    /// background reader. Returns once the connection is established and the
    /// reader is running.
    fn connect(endpoint: CoreEndpoint, connect_timeout: Duration) -> AppResult<Self> {
        let (reader, writer, closer) = transport::connect_split(&endpoint, connect_timeout)?;

        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let (subscribe_ack_tx, subscribe_ack_rx) = mpsc::sync_channel(1);
        let inner = Arc::new(Inner {
            endpoint: endpoint.clone(),
            writer: Mutex::new(writer),
            closer,
            pending: Mutex::new(HashMap::new()),
            event_tx,
            alive: AtomicBool::new(true),
            last_received_at_ms: AtomicU64::new(crate::services::common::now_unix_ms()),
            subscribe_ack_tx: Mutex::new(Some(subscribe_ack_tx)),
        });

        // Initial subscribe — this is what keeps the connection alive on the
        // kernel side.  We wait for the ack before returning (see below) so
        // that the first query cannot race with the subscribe handshake.
        let subscribe_frame = serde_json::json!({
            "type": "subscribe",
            "id": SUBSCRIBE_FRAME_ID,
            "events": [],
        });
        let subscribe_bytes = transport::serialize_frame(&subscribe_frame)?;
        push_debug_frame(DebugFrame {
            id: 0,
            at_ms: crate::services::common::now_unix_ms(),
            direction: "tx".to_string(),
            frame_type: "subscribe".to_string(),
            payload: subscribe_frame,
            elapsed_ms: None,
            error: None,
        });
        {
            let mut writer = recover_lock(&inner.writer);
            writer.write_all(&subscribe_bytes).map_err(|error| {
                AppError::from_io("failed to write IPC subscribe frame", &endpoint, error)
            })?;
            writer.flush().map_err(|error| {
                AppError::from_io("failed to flush IPC subscribe frame", &endpoint, error)
            })?;
        }
        // Spawn the reader on a dedicated OS thread: it blocks on `read_line`
        // for the connection's entire lifetime and should not occupy a tokio
        // blocking-pool slot.
        let reader_inner = Arc::clone(&inner);
        thread::Builder::new()
            .name("zero-ipc-reader".to_string())
            .spawn(move || reader_loop(reader, reader_inner))
            .map_err(|error| AppError::internal(format!("failed to spawn IPC reader: {error}")))?;

        // Wait for the kernel to acknowledge the subscribe frame.
        // Without this the first query can race with the subscribe
        // handshake — the kernel may not have registered the connection
        // as persistent yet, so a fast follow-up query can time out.
        match subscribe_ack_rx.recv_timeout(connect_timeout) {
            Ok(Ok(())) => {} // subscribe confirmed
            Ok(Err(error)) => {
                inner.mark_dead();
                return Err(error);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                inner.mark_dead();
                return Err(AppError {
                    code: "connection_closed",
                    message: "timed out waiting for kernel subscribe acknowledgement".to_string(),
                    details: Some(serde_json::json!({
                        "endpoint": endpoint.path,
                        "timeoutMs": connect_timeout.as_millis(),
                    })),
                });
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                inner.mark_dead();
                return Err(AppError::connection_closed(&endpoint));
            }
        }

        Ok(Self { inner })
    }

    /// Send a serialized request frame and await the matching response.
    ///
    /// `request_id` must be the `id` field embedded in `frame_bytes` (the
    /// protocol layer guarantees this). On timeout the pending slot is
    /// cleaned up so a late response is harmlessly dropped.
    pub async fn request(
        &self,
        frame_bytes: Vec<u8>,
        request_id: String,
        response_timeout: Duration,
    ) -> Result<Value, AppError> {
        if !self.inner.alive.load(Ordering::Acquire) {
            return Err(AppError::connection_closed(&self.inner.endpoint));
        }

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = recover_lock(&self.inner.pending);
            pending.insert(request_id.clone(), tx);
        }

        // Write the frame on a blocking thread — overlapped WriteFile blocks,
        // and we must not stall the async runtime. The pending slot is
        // already registered so a lightning-fast kernel response can still be
        // paired even while the write is completing.
        let frame_preview: String = String::from_utf8_lossy(&frame_bytes).into_owned();
        push_debug_frame(DebugFrame {
            id: 0,
            at_ms: crate::services::common::now_unix_ms(),
            direction: "tx".to_string(),
            frame_type: "multiplex".to_string(),
            payload: serde_json::json!({
                "requestId": request_id,
                "bytes": frame_bytes.len(),
                "preview": if frame_preview.len() > 200 {
                    format!("{}…", &frame_preview[..200])
                } else {
                    frame_preview
                },
            }),
            elapsed_ms: None,
            error: None,
        });
        let writer_inner = Arc::clone(&self.inner);
        let write_result = tauri::async_runtime::spawn_blocking(move || -> std::io::Result<()> {
            let mut writer = recover_lock(&writer_inner.writer);
            writer.write_all(&frame_bytes)?;
            writer.flush()
        })
        .await;

        let endpoint = self.inner.endpoint.clone();
        match write_result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                self.inner.remove_pending(&request_id);
                self.retire();
                return Err(AppError::from_io(
                    "failed to write IPC request",
                    &endpoint,
                    error,
                ));
            }
            Err(error) => {
                self.inner.remove_pending(&request_id);
                self.retire();
                return Err(AppError::internal(format!(
                    "IPC write worker failed: {error}"
                )));
            }
        }

        match timeout(response_timeout, rx).await {
            Ok(Ok(result)) => {
                push_debug_frame(DebugFrame {
                    id: 0,
                    at_ms: crate::services::common::now_unix_ms(),
                    direction: "rx".to_string(),
                    frame_type: "response".to_string(),
                    payload: serde_json::json!({
                        "requestId": request_id,
                        "status": "ok",
                    }),
                    elapsed_ms: None,
                    error: None,
                });
                result
            }
            Ok(Err(_)) => {
                // Sender dropped without sending — the reader tore the
                // connection down and drained pending waiters.
                push_debug_frame(DebugFrame {
                    id: 0,
                    at_ms: crate::services::common::now_unix_ms(),
                    direction: "rx".to_string(),
                    frame_type: "error".to_string(),
                    payload: serde_json::json!({
                        "requestId": request_id,
                        "reason": "connection_closed",
                    }),
                    elapsed_ms: None,
                    error: Some("connection closed (reader drained)".to_string()),
                });
                Err(AppError::connection_closed(&endpoint))
            }
            Err(_) => {
                push_debug_frame(DebugFrame {
                    id: 0,
                    at_ms: crate::services::common::now_unix_ms(),
                    direction: "rx".to_string(),
                    frame_type: "error".to_string(),
                    payload: serde_json::json!({
                        "requestId": request_id,
                        "timeoutMs": response_timeout.as_millis(),
                    }),
                    elapsed_ms: Some(response_timeout.as_millis() as u64),
                    error: Some(format!("timeout after {}ms", response_timeout.as_millis())),
                });
                self.inner.remove_pending(&request_id);
                Err(AppError {
                    code: "timeout",
                    message: "core IPC request timed out".to_string(),
                    details: Some(serde_json::json!({
                        "endpoint": endpoint.path,
                        "timeoutMs": response_timeout.as_millis() as u64,
                    })),
                })
            }
        }
    }

    /// Subscribe to the kernel event stream broadcast.
    pub fn subscribe_events(&self) -> broadcast::Receiver<Value> {
        self.inner.event_tx.subscribe()
    }

    /// Whether the connection is still usable.
    pub fn is_alive(&self) -> bool {
        self.inner.alive.load(Ordering::Acquire)
    }

    /// Endpoint path this connection is bound to.
    pub fn endpoint_path(&self) -> &str {
        &self.inner.endpoint.path
    }

    fn received_within(&self, window: Duration) -> bool {
        let now = crate::services::common::now_unix_ms();
        let last = self.inner.last_received_at_ms.load(Ordering::Acquire);
        now.saturating_sub(last) <= window.as_millis() as u64
    }

    /// Retire this shared transport so the manager opens a fresh subscribed
    /// connection on the next request.
    ///
    /// Event forwarders must also observe [`Self::is_alive`]: they keep a
    /// connection clone alongside the broadcast receiver, so retiring the
    /// transport alone cannot drop the sender and close that receiver.
    pub(crate) fn retire(&self) {
        self.inner.mark_dead();
    }
}

impl Inner {
    fn remove_pending(&self, id: &str) {
        recover_lock(&self.pending).remove(id);
    }

    fn mark_dead(&self) {
        if self.alive.swap(false, Ordering::AcqRel) {
            self.closer.close();
        }
    }
}

/// Background reader: classify each frame and route it.
///
/// Runs on its own OS thread (see [`MultiplexedConnection::connect`]).
/// Exits when [`transport::read_json_line`] reports an error — i.e. the pipe
/// broke — at which point it marks the connection dead and wakes every
/// pending waiter.
fn reader_loop(reader: KernelReader, inner: Arc<Inner>) {
    let mut buf = BufReader::new(reader);

    while inner.alive.load(Ordering::Acquire) {
        let frame = match transport::read_json_line(&mut buf, &inner.endpoint) {
            Ok(frame) => frame,
            Err(_) => break, // connection closed / IO error → tear down
        };
        inner
            .last_received_at_ms
            .store(crate::services::common::now_unix_ms(), Ordering::Release);

        let is_response = frame
            .as_object()
            .is_some_and(|object| object.contains_key("ok"));
        let frame_id = frame
            .get("id")
            .or_else(|| frame.get("request_id"))
            .or_else(|| frame.get("requestId"))
            .and_then(|value| value.as_str())
            .map(|value| value.to_string());

        if is_response {
            // If this is the subscribe ack, signal the connect() caller
            // and then drop it — no query waiter exists for this id.
            if frame_id.as_deref() == Some(SUBSCRIBE_FRAME_ID) {
                push_debug_frame(DebugFrame {
                    id: 0,
                    at_ms: crate::services::common::now_unix_ms(),
                    direction: "rx".to_string(),
                    frame_type: "subscribe-ack".to_string(),
                    payload: frame.clone(),
                    elapsed_ms: None,
                    error: None,
                });
                if let Some(tx) = recover_lock(&inner.subscribe_ack_tx).take() {
                    let _ = tx.send(validate_subscribe_ack(&frame));
                }
                continue;
            }

            // Response frame: pair by id with the waiting waiter, if any.
            if let Some(id) = frame_id.as_deref() {
                let ok = frame.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                let sender = recover_lock(&inner.pending).remove(id);
                let matched = sender.is_some();
                if let Some(sender) = sender {
                    let _ = sender.send(Ok(frame));
                }
                push_debug_frame(DebugFrame {
                    id: 0,
                    at_ms: crate::services::common::now_unix_ms(),
                    direction: "rx".to_string(),
                    frame_type: "response".to_string(),
                    payload: serde_json::json!({
                        "requestId": id,
                        "matched": matched,
                        "ok": ok,
                    }),
                    elapsed_ms: None,
                    error: None,
                });
            } else {
                // No id on this response — log a snippet so we can identify
                // what the kernel is sending.
                let snippet: String =
                    serde_json::to_string(&frame).unwrap_or_else(|_| "<invalid>".to_string());
                let preview: String = if snippet.len() > 200 {
                    format!("{}…", &snippet[..200])
                } else {
                    snippet
                };
                push_debug_frame(DebugFrame {
                    id: 0,
                    at_ms: crate::services::common::now_unix_ms(),
                    direction: "rx".to_string(),
                    frame_type: "orphan-response".to_string(),
                    payload: serde_json::json!({ "preview": preview }),
                    elapsed_ms: None,
                    error: Some("no matching request id".to_string()),
                });
            }
            // Response with no matching id → drop.
        } else {
            // Event frame: fan out to every subscriber. No subscriber → ignore.
            push_debug_frame(DebugFrame {
                id: 0,
                at_ms: crate::services::common::now_unix_ms(),
                direction: "rx".to_string(),
                frame_type: "event".to_string(),
                payload: frame.clone(),
                elapsed_ms: None,
                error: None,
            });
            let _ = inner.event_tx.send(frame);
        }
    }

    // Connection is gone: mark dead and wake every pending waiter so they
    // don't block until their own timeout.
    inner.alive.store(false, Ordering::Release);
    let drained: HashMap<String, oneshot::Sender<Result<Value, AppError>>> = {
        let mut guard = recover_lock(&inner.pending);
        std::mem::take(&mut *guard)
    };
    let endpoint = inner.endpoint.clone();
    for (_, sender) in drained {
        let _ = sender.send(Err(AppError::connection_closed(&endpoint)));
    }
}

fn validate_subscribe_ack(frame: &Value) -> AppResult<()> {
    if frame.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(AppError::core_response(frame.clone()));
    }
    if frame.get("api_id").and_then(Value::as_str) != Some("zero.api.v1") {
        return Err(AppError {
            code: "invalid_response",
            message: "core IPC subscribe acknowledgement has an invalid api_id".to_string(),
            details: Some(frame.clone()),
        });
    }
    if frame.get("result").and_then(Value::as_str) != Some("subscribed") {
        return Err(AppError {
            code: "invalid_response",
            message: "core IPC subscribe acknowledgement did not confirm subscription".to_string(),
            details: Some(frame.clone()),
        });
    }
    Ok(())
}

// ── Global connection manager ───────────────────────────────────────

struct ManagedConnection {
    endpoint_path: String,
    conn: MultiplexedConnection,
}

static MANAGER: LazyLock<Mutex<Option<ManagedConnection>>> = LazyLock::new(|| Mutex::new(None));
/// Serializes cold connects without holding [`MANAGER`] across blocking pipe
/// I/O. Dropping a losing `MultiplexedConnection` is not sufficient to close
/// its pipe because the reader thread owns another `Arc`, so concurrent cold
/// connects would otherwise leave orphan kernel subscriptions behind.
static CONNECT_GATE: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Return the live multiplexed connection for `endpoint`, creating one if
/// none exists, the cached one is dead, or it is bound to a different path.
///
/// The fast path (cache hit) holds the manager lock only long enough to clone
/// the cached connection. The slow path is serialized by [`CONNECT_GATE`]
/// and rechecks the cache before opening the pipe. This keeps blocking pipe
/// I/O outside [`MANAGER`] while guaranteeing that only one reader thread and
/// kernel subscription are created for a cold endpoint.
pub fn get_or_connect(
    endpoint: CoreEndpoint,
    connect_timeout: Duration,
) -> AppResult<MultiplexedConnection> {
    let path = endpoint.path.clone();

    // ── Fast path: check the cache without blocking on connect ──
    {
        let guard = recover_lock(&MANAGER);
        if let Some(managed) = guard.as_ref() {
            if managed.endpoint_path == path && managed.conn.is_alive() {
                return Ok(managed.conn.clone());
            }
        }
    }

    // Only one caller may perform the blocking subscribe handshake. Other
    // callers wait here, then reuse the connection published by the winner.
    let _connect_guard = recover_lock(&CONNECT_GATE);

    // A caller may have populated the cache while we waited for the gate.
    {
        let guard = recover_lock(&MANAGER);
        if let Some(managed) = guard.as_ref() {
            if managed.endpoint_path == path && managed.conn.is_alive() {
                return Ok(managed.conn.clone());
            }
        }
    }

    let conn = MultiplexedConnection::connect(endpoint, connect_timeout)?;

    // Publish while the connect gate is still held, so no concurrent cold
    // caller can create a second reader/subscription.
    let mut guard = recover_lock(&MANAGER);
    *guard = Some(ManagedConnection {
        endpoint_path: path,
        conn: conn.clone(),
    });
    Ok(conn)
}

/// Drop the cached connection, if any. Called when the kernel is stopped so
/// the next request reconnects cleanly instead of reusing a dead handle.
pub fn reset() {
    // Wait for an in-flight connect to finish before clearing the cache. This
    // prevents an old-endpoint connection from being published after reset.
    let _connect_guard = recover_lock(&CONNECT_GATE);
    let mut guard = recover_lock(&MANAGER);
    if let Some(managed) = guard.take() {
        managed.conn.retire();
    }
}

/// Verify IPC health over the shared multiplexed channel.
///
/// Recent inbound traffic already proves that the shared channel is alive,
/// so no ping is sent while events or responses are flowing. If an idle
/// channel fails its ping, retire it and make one bounded reconnect attempt
/// using a new persistent subscribe connection. This never uses the
/// single-shot transport.
///
/// Returns `true` when the check had to rebuild the shared connection.
pub async fn ensure_healthy(
    endpoint: CoreEndpoint,
    timeout: Duration,
    activity_window: Duration,
) -> AppResult<bool> {
    let conn = connect_for_health(endpoint.clone(), timeout).await?;
    if conn.received_within(activity_window) {
        return Ok(false);
    }

    match ping_connection(conn, timeout).await {
        Ok(()) => Ok(false),
        Err(_) => ping_once(endpoint, timeout).await.map(|()| true),
    }
}

async fn ping_once(endpoint: CoreEndpoint, timeout: Duration) -> AppResult<()> {
    let conn = connect_for_health(endpoint, timeout).await?;
    ping_connection(conn, timeout).await
}

async fn connect_for_health(
    endpoint: CoreEndpoint,
    timeout: Duration,
) -> AppResult<MultiplexedConnection> {
    tauri::async_runtime::spawn_blocking(move || get_or_connect(endpoint, timeout))
        .await
        .map_err(|error| AppError::internal(format!("IPC health connect worker failed: {error}")))?
}

async fn ping_connection(conn: MultiplexedConnection, timeout: Duration) -> AppResult<()> {
    static NEXT_HEALTH_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

    let request_id = format!(
        "znet-sink-health-{}",
        NEXT_HEALTH_ID.fetch_add(1, Ordering::Relaxed)
    );
    let frame = serde_json::json!({
        "type": "ping",
        "id": request_id,
    });
    let frame_bytes = transport::serialize_frame(&frame)?;
    let response = match conn.request(frame_bytes, request_id.clone(), timeout).await {
        Ok(response) => response,
        Err(error) => {
            conn.retire();
            return Err(error);
        }
    };

    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        conn.retire();
        return Err(AppError::core_response(response));
    }
    let response_id = response
        .get("id")
        .or_else(|| response.get("request_id"))
        .or_else(|| response.get("requestId"))
        .and_then(Value::as_str);
    if response_id != Some(request_id.as_str()) {
        conn.retire();
        return Err(AppError::internal(
            "kernel IPC health response id did not match the request",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_subscribe_ack;
    use serde_json::json;

    #[test]
    fn subscribe_ack_requires_the_documented_success_envelope() {
        validate_subscribe_ack(&json!({
            "api_id": "zero.api.v1",
            "ok": true,
            "id": "znet-sink-subscribe",
            "result": "subscribed"
        }))
        .unwrap();
    }

    #[test]
    fn subscribe_ack_preserves_kernel_rejection() {
        let error = validate_subscribe_ack(&json!({
            "api_id": "zero.api.v1",
            "ok": false,
            "id": "znet-sink-subscribe",
            "error": { "code": "permission_denied", "message": "denied" }
        }))
        .unwrap_err();
        assert_eq!(error.code, "core_error");
        assert_eq!(error.message, "denied");
    }

    #[test]
    fn subscribe_ack_rejects_wrong_api_or_result() {
        for frame in [
            json!({"api_id":"other", "ok":true, "result":"subscribed"}),
            json!({"api_id":"zero.api.v1", "ok":true, "result":"unexpected"}),
        ] {
            assert_eq!(
                validate_subscribe_ack(&frame).unwrap_err().code,
                "invalid_response"
            );
        }
    }
}
