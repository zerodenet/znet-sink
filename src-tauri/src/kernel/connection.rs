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
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::{broadcast, oneshot};
use tokio::time::timeout;

use crate::errors::{AppError, AppResult};
use crate::kernel::transport::{self, KernelReader, KernelWriter};
use crate::models::core::CoreEndpoint;

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
}

impl MultiplexedConnection {
    /// Open a connection, send the initial `subscribe` frame, and spawn the
    /// background reader. Returns once the connection is established and the
    /// reader is running.
    fn connect(endpoint: CoreEndpoint, connect_timeout: Duration) -> AppResult<Self> {
        let (reader, writer) = transport::connect_split(&endpoint, connect_timeout)?;

        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let inner = Arc::new(Inner {
            endpoint: endpoint.clone(),
            writer: Mutex::new(writer),
            pending: Mutex::new(HashMap::new()),
            event_tx,
            alive: AtomicBool::new(true),
        });

        // Initial subscribe — this is what keeps the connection alive on the
        // kernel side. Its ack (`{"ok":true,...}`) is classified as a
        // response and dropped (nobody awaits this id), which is fine.
        let subscribe_frame = serde_json::json!({
            "type": "subscribe",
            "id": SUBSCRIBE_FRAME_ID,
            "events": [],
        });
        let subscribe_bytes = transport::serialize_frame(&subscribe_frame)?;
        {
            let mut writer = inner
                .writer
                .lock()
                .expect("IPC writer mutex poisoned");
            writer
                .write_all(&subscribe_bytes)
                .map_err(|error| {
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
            let mut pending = self.inner.pending.lock().expect("IPC pending mutex poisoned");
            pending.insert(request_id.clone(), tx);
        }

        // Write the frame on a blocking thread — overlapped WriteFile blocks,
        // and we must not stall the async runtime. The pending slot is
        // already registered so a lightning-fast kernel response can still be
        // paired even while the write is completing.
        let writer_inner = Arc::clone(&self.inner);
        let write_result = tauri::async_runtime::spawn_blocking(
            move || -> std::io::Result<()> {
                let mut writer = writer_inner
                    .writer
                    .lock()
                    .expect("IPC writer mutex poisoned");
                writer.write_all(&frame_bytes)?;
                writer.flush()
            },
        )
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
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                // Sender dropped without sending — the reader tore the
                // connection down and drained pending waiters.
                Err(AppError::connection_closed(&endpoint))
            }
            Err(_) => {
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

        if is_response {
            // Response frame: pair by id with the waiting waiter, if any.
            if let Some(id) = frame
                .get("id")
                .or_else(|| frame.get("request_id"))
                .or_else(|| frame.get("requestId"))
                .and_then(|value| value.as_str())
            {
                if let Some(sender) = inner
                    .pending
                    .lock()
                    .expect("IPC pending mutex poisoned")
                    .remove(id)
                {
                    // Receiver may have timed out already — ignore send error.
                    let _ = sender.send(Ok(frame));
                }
            }
            // Response with no matching id (e.g. the subscribe ack) → drop.
        } else {
            // Event frame: fan out to every subscriber. No subscriber → ignore.
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
/// Safe to call from a blocking context (it performs blocking connect I/O
/// when a new connection is needed; the cache-hit path is lock + check +
/// clone).
pub fn get_or_connect(
    endpoint: CoreEndpoint,
    connect_timeout: Duration,
) -> AppResult<MultiplexedConnection> {
    let path = endpoint.path.clone();
    let mut guard = MANAGER.lock().expect("connection manager mutex poisoned");

    if let Some(managed) = guard.as_ref() {
        if managed.endpoint_path == path && managed.conn.is_alive() {
            return Ok(managed.conn.clone());
        }
        // Stale or wrong endpoint — drop the cache and rebuild.
    }

    let conn = MultiplexedConnection::connect(endpoint, connect_timeout)?;
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
