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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, LazyLock, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::{broadcast, oneshot};
use tokio::time::timeout;

use crate::errors::{AppError, AppResult};
use crate::kernel::transport::{self, KernelReader, KernelWriter};
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
    pending: Mutex<HashMap<String, oneshot::Sender<Result<Value, AppError>>>>,
    event_tx: broadcast::Sender<Value>,
    alive: AtomicBool,
    /// Set during `connect()` so the reader can signal when the subscribe
    /// acknowledgement arrives.  Taken by the reader on first response
    /// frame whose `id` matches [`SUBSCRIBE_FRAME_ID`], then read by
    /// `connect()` before it returns.
    subscribe_ack_tx: Mutex<Option<mpsc::SyncSender<()>>>,
}

impl MultiplexedConnection {
    /// Open a connection, send the initial `subscribe` frame, and spawn the
    /// background reader. Returns once the connection is established and the
    /// reader is running.
    fn connect(endpoint: CoreEndpoint, connect_timeout: Duration) -> AppResult<Self> {
        let (reader, writer) = transport::connect_split(&endpoint, connect_timeout)?;

        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let (subscribe_ack_tx, subscribe_ack_rx) = mpsc::sync_channel(1);
        let inner = Arc::new(Inner {
            endpoint: endpoint.clone(),
            writer: Mutex::new(writer),
            pending: Mutex::new(HashMap::new()),
            event_tx,
            alive: AtomicBool::new(true),
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
        {
            let mut writer = inner.writer.lock().expect("IPC writer mutex poisoned");
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
            Ok(()) => {} // subscribe confirmed
            Err(mpsc::RecvTimeoutError::Timeout) => {
                inner.alive.store(false, Ordering::Release);
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
                inner.alive.store(false, Ordering::Release);
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
            let mut pending = self
                .inner
                .pending
                .lock()
                .expect("IPC pending mutex poisoned");
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
            direction: "tx",
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
            let mut writer = writer_inner
                .writer
                .lock()
                .expect("IPC writer mutex poisoned");
            writer.write_all(&frame_bytes)?;
            writer.flush()
        })
        .await;

        let endpoint = self.inner.endpoint.clone();
        match write_result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                self.inner.remove_pending(&request_id);
                self.mark_dead();
                return Err(AppError::from_io(
                    "failed to write IPC request",
                    &endpoint,
                    error,
                ));
            }
            Err(error) => {
                self.inner.remove_pending(&request_id);
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
                    direction: "rx",
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
                    direction: "rx",
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
                    direction: "rx",
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

    fn mark_dead(&self) {
        self.inner.alive.store(false, Ordering::Release);
    }
}

impl Inner {
    fn remove_pending(&self, id: &str) {
        self.pending
            .lock()
            .expect("IPC pending mutex poisoned")
            .remove(id);
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
                    direction: "rx",
                    frame_type: "subscribe-ack".to_string(),
                    payload: serde_json::json!({ "subscribed": true }),
                    elapsed_ms: None,
                    error: None,
                });
                if let Some(tx) = inner
                    .subscribe_ack_tx
                    .lock()
                    .expect("IPC subscribe ack mutex poisoned")
                    .take()
                {
                    let _ = tx.send(());
                }
                continue;
            }

            // Response frame: pair by id with the waiting waiter, if any.
            if let Some(id) = frame_id.as_deref() {
                let ok = frame.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                let sender = inner
                    .pending
                    .lock()
                    .expect("IPC pending mutex poisoned")
                    .remove(id);
                let matched = sender.is_some();
                if let Some(sender) = sender {
                    let _ = sender.send(Ok(frame));
                }
                push_debug_frame(DebugFrame {
                    id: 0,
                    at_ms: crate::services::common::now_unix_ms(),
                    direction: "rx",
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
                    direction: "rx",
                    frame_type: "orphan-response".to_string(),
                    payload: serde_json::json!({ "preview": preview }),
                    elapsed_ms: None,
                    error: Some("no matching request id".to_string()),
                });
            }
            // Response with no matching id → drop.
        } else {
            // Event frame: fan out to every subscriber. No subscriber → ignore.
            let event_type = frame
                .get("event_type")
                .or_else(|| frame.get("type"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            push_debug_frame(DebugFrame {
                id: 0,
                at_ms: crate::services::common::now_unix_ms(),
                direction: "rx",
                frame_type: "event".to_string(),
                payload: serde_json::json!({
                    "eventType": event_type,
                }),
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
        let mut guard = inner.pending.lock().expect("IPC pending mutex poisoned");
        std::mem::take(&mut *guard)
    };
    let endpoint = inner.endpoint.clone();
    for (_, sender) in drained {
        let _ = sender.send(Err(AppError::connection_closed(&endpoint)));
    }
}

// ── Global connection manager ───────────────────────────────────────

struct ManagedConnection {
    endpoint_path: String,
    conn: MultiplexedConnection,
}

static MANAGER: LazyLock<Mutex<Option<ManagedConnection>>> = LazyLock::new(|| Mutex::new(None));

/// Return the live multiplexed connection for `endpoint`, creating one if
/// none exists, the cached one is dead, or it is bound to a different path.
///
/// The fast path (cache hit) holds the lock only long enough to clone the
/// cached connection.  The slow path (cold start or dead connection) drops
/// the lock before calling [`MultiplexedConnection::connect`] (which blocks
/// for up to `connect_timeout` waiting for the subscribe ack) so that
/// concurrent callers do not queue up behind a held lock and time out
/// before the connection is ready.
///
/// When two callers race to create the first connection, the second one to
/// finish simply drops its connection and returns the cached one — only one
/// multiplexed pipe is ever open at a time.
pub fn get_or_connect(
    endpoint: CoreEndpoint,
    connect_timeout: Duration,
) -> AppResult<MultiplexedConnection> {
    let path = endpoint.path.clone();

    // ── Fast path: check the cache without blocking on connect ──
    {
        let guard = MANAGER.lock().expect("connection manager mutex poisoned");
        if let Some(managed) = guard.as_ref() {
            if managed.endpoint_path == path && managed.conn.is_alive() {
                return Ok(managed.conn.clone());
            }
        }
    } // lock dropped here — connect() below does NOT hold it

    let conn = MultiplexedConnection::connect(endpoint, connect_timeout)?;

    // ── Publish the new connection, preferring a concurrent winner ──
    let mut guard = MANAGER.lock().expect("connection manager mutex poisoned");
    if let Some(managed) = guard.as_ref() {
        if managed.endpoint_path == path && managed.conn.is_alive() {
            // Another thread beat us to it — use its connection instead.
            return Ok(managed.conn.clone());
        }
    }
    *guard = Some(ManagedConnection {
        endpoint_path: path,
        conn: conn.clone(),
    });
    Ok(conn)
}

/// Drop the cached connection, if any. Called when the kernel is stopped so
/// the next request reconnects cleanly instead of reusing a dead handle.
pub fn reset() {
    if let Ok(mut guard) = MANAGER.lock() {
        *guard = None;
    }
}
