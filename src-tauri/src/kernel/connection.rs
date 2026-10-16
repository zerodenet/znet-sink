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
//! The initial subscribe acknowledgement is validated separately. Other
//! response frames without a matching waiter are dropped silently.
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
use std::time::{Duration, Instant};

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

use crate::errors::{AppError, AppResult};
use crate::kernel::transport::{self, KernelCloser, KernelReader, KernelWriter};
use crate::models::core::CoreEndpoint;
use crate::models::debug::{capture_payload, push_debug_frame, DebugFrame, DebugTelemetry};

/// `id` used for the initial `subscribe` frame. Distinct from the
/// `znet-sink-<n>` request ids so it can never collide with a pending
/// request, and its ack is simply dropped by the reader.
const SUBSCRIBE_FRAME_ID: &str = "znet-sink-subscribe";

mod scoped;
pub(crate) use scoped::ScopedConnection;
pub(crate) mod event_filter;
#[cfg(all(test, unix))]
#[path = "connection/handshake_tests.rs"]
mod handshake_tests;
#[cfg(all(test, unix))]
#[path = "connection/security_tests.rs"]
mod security_tests;

/// Broadcast channel capacity for kernel events.
///
/// Generous on purpose: traffic stats and flow events can burst. A slow
/// consumer falls behind and [`broadcast::Receiver`] reports
/// [`Lagged`](broadcast::error::RecvError::Lagged), which the event
/// forwarders tolerate.
const EVENT_CHANNEL_CAPACITY: usize = 128;
const MAX_EVENT_BYTES: usize = 256 * 1024;
const MAX_PENDING_REQUESTS: usize = 64;

pub(crate) fn is_gap_event(frame: &Value) -> bool {
    frame.get("event_type").and_then(Value::as_str) == Some("znet.transport_gap")
}

struct PendingRequest {
    nonce: u64,
    sender: oneshot::Sender<Result<Value, AppError>>,
}
struct PendingSlot {
    inner: Arc<Inner>,
    id: String,
    nonce: u64,
}
impl Drop for PendingSlot {
    fn drop(&mut self) {
        let mut pending = recover_lock(&self.inner.pending);
        if pending
            .get(&self.id)
            .is_some_and(|request| request.nonce == self.nonce)
        {
            pending.remove(&self.id);
        }
    }
}

/// A shared multiplexed connection. Cheap to clone (internally `Arc`).
#[derive(Clone)]
pub struct MultiplexedConnection {
    inner: Arc<Inner>,
}

// A typed reader termination cannot be confused with a kernel rejection
// that happens to contain an error code resembling a closed connection.
enum SubscribeNotice {
    Acknowledgement(AppResult<()>),
    ReaderStopped(AppError),
}
enum HandshakeFailure {
    ClosedBeforeAck(AppError),
    Terminal(AppError),
}
impl From<AppError> for HandshakeFailure {
    fn from(error: AppError) -> Self {
        Self::Terminal(error)
    }
}
const MAX_COLD_HANDSHAKE_ATTEMPTS: usize = 3;
fn subscribe_timeout(endpoint: &CoreEndpoint, budget: Duration) -> AppError {
    AppError {
        code: "connection_closed",
        message: "timed out waiting for kernel subscribe acknowledgement".into(),
        details: Some(serde_json::json!({"endpoint":endpoint.path,"timeoutMs":budget.as_millis()})),
    }
}

struct Inner {
    endpoint: CoreEndpoint,
    writer: Mutex<KernelWriter>,
    closer: KernelCloser,
    pending: Mutex<HashMap<String, PendingRequest>>,
    next_pending_nonce: AtomicU64,
    event_tx: broadcast::Sender<Value>,
    alive: AtomicBool,
    last_received_at_ms: AtomicU64,
    /// Set during `connect()` so the reader can signal when the subscribe
    /// acknowledgement arrives.  Taken by the reader on first response
    /// frame whose `id` matches [`SUBSCRIBE_FRAME_ID`], then read by
    /// `connect()` before it returns.
    subscribe_ack_tx: Mutex<Option<mpsc::SyncSender<SubscribeNotice>>>,
}

impl MultiplexedConnection {
    /// Open a connection, send the initial `subscribe` frame, and spawn the
    /// background reader. Returns once the connection is established and the
    /// reader is running. Before acknowledgement only, an observed closed
    /// transport can rebuild the subscribe handshake up to three attempts.
    /// All attempts share one deadline; existing platform socket-connect
    /// limits remain owned by transport (not a hard OS syscall deadline).
    fn connect(endpoint: CoreEndpoint, connect_timeout: Duration) -> AppResult<Self> {
        let deadline = Instant::now()
            .checked_add(connect_timeout)
            .ok_or_else(|| AppError::invalid_argument("IPC connect timeout is out of range"))?;
        for attempt in 0..MAX_COLD_HANDSHAKE_ATTEMPTS {
            if Instant::now() >= deadline {
                return Err(subscribe_timeout(&endpoint, connect_timeout));
            }
            match Self::connect_once(endpoint.clone(), deadline, connect_timeout) {
                Ok(connection) => return Ok(connection),
                Err(HandshakeFailure::ClosedBeforeAck(error)) => {
                    if attempt + 1 == MAX_COLD_HANDSHAKE_ATTEMPTS || Instant::now() >= deadline {
                        return Err(error);
                    }
                    // Only subscribe was sent. No business frame, binding, or
                    // permission has been accepted, and nothing is replayed.
                }
                Err(HandshakeFailure::Terminal(error)) => return Err(error),
            }
        }
        unreachable!("bounded cold handshake loop always returns")
    }

    fn connect_once(
        endpoint: CoreEndpoint,
        deadline: Instant,
        connect_timeout: Duration,
    ) -> Result<Self, HandshakeFailure> {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| subscribe_timeout(&endpoint, connect_timeout))?;
        let (reader, writer, closer) = transport::connect_split(&endpoint, remaining)?;

        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let (subscribe_ack_tx, subscribe_ack_rx) = mpsc::sync_channel(1);
        let inner = Arc::new(Inner {
            endpoint: endpoint.clone(),
            writer: Mutex::new(writer),
            closer,
            pending: Mutex::new(HashMap::new()),
            next_pending_nonce: AtomicU64::new(1),
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
            telemetry: None,
            elapsed_ms: None,
            error: None,
        });
        {
            let mut writer = recover_lock(&inner.writer);
            let mut remaining_bytes = subscribe_bytes.as_slice();
            while !remaining_bytes.is_empty() {
                let remaining = deadline
                    .checked_duration_since(Instant::now())
                    .filter(|value| !value.is_zero())
                    .ok_or_else(|| subscribe_timeout(&endpoint, connect_timeout))?;
                writer.set_write_timeout(remaining).map_err(|error| {
                    AppError::from_io(
                        "failed to set IPC subscribe write timeout",
                        &endpoint,
                        error,
                    )
                })?;
                let written = match writer.write(remaining_bytes) {
                    Ok(written) => written,
                    // Preserve write_all's signal-interruption behavior while
                    // retaining the same absolute handshake deadline.
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => {
                        return Err(AppError::from_io(
                            "failed to write IPC subscribe frame",
                            &endpoint,
                            error,
                        )
                        .into());
                    }
                };
                if written == 0 {
                    return Err(AppError::from_io(
                        "failed to write IPC subscribe frame",
                        &endpoint,
                        std::io::Error::new(
                            std::io::ErrorKind::WriteZero,
                            "subscribe write made no progress",
                        ),
                    )
                    .into());
                }
                remaining_bytes = &remaining_bytes[written..];
            }
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .filter(|value| !value.is_zero())
                .ok_or_else(|| subscribe_timeout(&endpoint, connect_timeout))?;
            writer.set_write_timeout(remaining).map_err(|error| {
                AppError::from_io(
                    "failed to set IPC subscribe flush timeout",
                    &endpoint,
                    error,
                )
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
        let remaining = deadline.saturating_duration_since(Instant::now());
        match subscribe_ack_rx.recv_timeout(remaining) {
            Ok(SubscribeNotice::Acknowledgement(Ok(()))) => {}
            Ok(SubscribeNotice::Acknowledgement(Err(error))) => {
                inner.mark_dead();
                return Err(HandshakeFailure::Terminal(error));
            }
            Ok(SubscribeNotice::ReaderStopped(error)) => {
                inner.mark_dead();
                return Err(if error.code == "connection_closed" {
                    HandshakeFailure::ClosedBeforeAck(error)
                } else {
                    HandshakeFailure::Terminal(error)
                });
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                inner.mark_dead();
                return Err(HandshakeFailure::Terminal(subscribe_timeout(
                    &endpoint,
                    connect_timeout,
                )));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                inner.mark_dead();
                return Err(HandshakeFailure::Terminal(AppError::connection_closed(
                    &endpoint,
                )));
            }
        }

        if Instant::now() >= deadline {
            inner.mark_dead();
            return Err(HandshakeFailure::Terminal(subscribe_timeout(
                &endpoint,
                connect_timeout,
            )));
        }
        if let Err(error) = recover_lock(&inner.writer).set_write_timeout(connect_timeout) {
            inner.mark_dead();
            return Err(
                AppError::from_io("failed to restore IPC write timeout", &endpoint, error).into(),
            );
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
        if frame_bytes.len() > transport::MAX_FRAME_BYTES
            || request_id.is_empty()
            || request_id.len() > 128
            || request_id == SUBSCRIBE_FRAME_ID
        {
            return Err(AppError::invalid_argument(
                "IPC request exceeds the frame or identity budget",
            ));
        }
        let deadline = tokio::time::Instant::now() + response_timeout;

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = recover_lock(&self.inner.pending);
            if pending.len() >= MAX_PENDING_REQUESTS || pending.contains_key(&request_id) {
                return Err(AppError {
                    code: "busy",
                    message: "IPC pending request budget or duplicate identity rejected".into(),
                    details: None,
                });
            }
            let nonce = self
                .inner
                .next_pending_nonce
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .map_err(|_| AppError::internal("IPC request identity budget exhausted"))?;
            pending.insert(request_id.clone(), PendingRequest { nonce, sender: tx });
        }
        let nonce = recover_lock(&self.inner.pending)
            .get(&request_id)
            .map(|request| request.nonce);
        // The reader may already have delivered and removed this slot.
        let _pending_slot = nonce.map(|nonce| PendingSlot {
            inner: self.inner.clone(),
            id: request_id.clone(),
            nonce,
        });

        // Write the frame on a blocking thread — overlapped WriteFile blocks,
        // and we must not stall the async runtime. The pending slot is
        // already registered so a lightning-fast kernel response can still be
        // paired even while the write is completing.
        push_debug_frame(DebugFrame {
            id: 0,
            at_ms: crate::services::common::now_unix_ms(),
            direction: "tx".to_string(),
            frame_type: "multiplex".to_string(),
            payload: serde_json::json!({
                "requestId": request_id,
                "bytes": frame_bytes.len(),
            }),
            telemetry: None,
            elapsed_ms: None,
            error: None,
        });
        let writer_inner = Arc::clone(&self.inner);
        let write_result = tokio::time::timeout_at(
            deadline,
            tauri::async_runtime::spawn_blocking(move || -> std::io::Result<()> {
                let mut writer = recover_lock(&writer_inner.writer);
                writer.write_all(&frame_bytes)?;
                writer.flush()
            }),
        )
        .await;

        let endpoint = self.inner.endpoint.clone();
        match write_result {
            Ok(Ok(Ok(()))) => {}
            Ok(Ok(Err(error))) => {
                self.inner.remove_pending(&request_id);
                self.retire();
                return Err(AppError::from_io(
                    "failed to write IPC request",
                    &endpoint,
                    error,
                ));
            }
            Ok(Err(error)) => {
                self.inner.remove_pending(&request_id);
                self.retire();
                return Err(AppError::internal(format!(
                    "IPC write worker failed: {error}"
                )));
            }
            Err(_) => {
                return Err(AppError {code:"timeout",message:"IPC request write result is unconfirmed; the in-flight write was not cancelled".into(),details:None});
            }
        }

        match tokio::time::timeout_at(deadline, rx).await {
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
                    telemetry: None,
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
                    telemetry: None,
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
                    telemetry: None,
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

    pub(crate) fn received_within(&self, window: Duration) -> bool {
        let now = crate::services::common::now_unix_ms();
        let last = self.inner.last_received_at_ms.load(Ordering::Acquire);
        now.saturating_sub(last) <= window.as_millis() as u64
    }

    /// Whether another request is still legitimately waiting for a response.
    ///
    /// A long-running command such as `tun.start` can temporarily delay
    /// otherwise cheap reads on kernels that serialize command handling. A
    /// read timeout must not retire the shared transport while that command is
    /// still inside its own response deadline.
    pub(crate) fn has_pending_requests(&self) -> bool {
        !recover_lock(&self.inner.pending).is_empty()
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
    let mut read_failure = None;

    while inner.alive.load(Ordering::Acquire) {
        let frame = match transport::read_json_line(&mut buf, &inner.endpoint) {
            Ok(frame) => frame,
            Err(error) => {
                read_failure = Some(error);
                break;
            }
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
                    payload: capture_payload(&frame),
                    telemetry: None,
                    elapsed_ms: None,
                    error: None,
                });
                if let Some(tx) = recover_lock(&inner.subscribe_ack_tx).take() {
                    let _ = tx.send(SubscribeNotice::Acknowledgement(validate_subscribe_ack(
                        &frame,
                    )));
                }
                continue;
            }

            // Response frame: pair by id with the waiting waiter, if any.
            if let Some(id) = frame_id.as_deref() {
                let ok = frame.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                let sender = recover_lock(&inner.pending).remove(id);
                let matched = sender.is_some();
                if let Some(sender) = sender {
                    let _ = sender.sender.send(Ok(frame));
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
                    telemetry: None,
                    elapsed_ms: None,
                    error: None,
                });
            } else {
                // No id on this response — log a snippet so we can identify
                // what the kernel is sending.
                let snippet: String = serde_json::to_string(&capture_payload(&frame))
                    .unwrap_or_else(|_| "<invalid>".to_string());
                let preview: String = if snippet.len() > 200 {
                    format!("{}…", snippet.chars().take(200).collect::<String>())
                } else {
                    snippet
                };
                push_debug_frame(DebugFrame {
                    id: 0,
                    at_ms: crate::services::common::now_unix_ms(),
                    direction: "rx".to_string(),
                    frame_type: "orphan-response".to_string(),
                    payload: serde_json::json!({ "preview": preview }),
                    telemetry: None,
                    elapsed_ms: None,
                    error: Some("no matching request id".to_string()),
                });
            }
            // Response with no matching id → drop.
        } else {
            if serde_json::to_vec(&frame).map_or(true, |bytes| bytes.len() > MAX_EVENT_BYTES) {
                let _ = inner.event_tx.send(serde_json::json!({"event_type":"znet.transport_gap","reason":"event_byte_budget"}));
                continue;
            }
            // Event frame: fan out to every subscriber. No subscriber → ignore.
            push_debug_frame(DebugFrame {
                id: 0,
                at_ms: crate::services::common::now_unix_ms(),
                direction: "rx".to_string(),
                frame_type: "event".to_string(),
                payload: capture_payload(&frame),
                telemetry: DebugTelemetry::from_event(&frame),
                elapsed_ms: None,
                error: None,
            });
            let _ = inner.event_tx.send(frame);
        }
    }

    // Connection is gone: mark dead and wake every pending waiter so they
    // don't block until their own timeout.
    inner.mark_dead();
    // Inner itself still owns this sender. Taking it on reader termination
    // is essential: dropping only the reader Arc would leave connect() waiting
    // for the entire acknowledgement timeout after an immediate EOF.
    if let Some(tx) = recover_lock(&inner.subscribe_ack_tx).take() {
        let _ = tx.send(SubscribeNotice::ReaderStopped(
            read_failure.unwrap_or_else(|| AppError::connection_closed(&inner.endpoint)),
        ));
    }
    let drained: HashMap<String, PendingRequest> = {
        let mut guard = recover_lock(&inner.pending);
        std::mem::take(&mut *guard)
    };
    let endpoint = inner.endpoint.clone();
    for (_, sender) in drained {
        let _ = sender
            .sender
            .send(Err(AppError::connection_closed(&endpoint)));
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

type EndpointKey = (&'static str, String);

static MANAGER: LazyLock<Mutex<HashMap<EndpointKey, MultiplexedConnection>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// Serializes cold connects without holding [`MANAGER`] across blocking pipe
/// I/O. Dropping a losing `MultiplexedConnection` is not sufficient to close
/// its pipe because the reader thread owns another `Arc`, so concurrent cold
/// connects would otherwise leave orphan kernel subscriptions behind.
static CONNECT_GATE: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Return the live multiplexed connection for `endpoint`, creating one if
/// none exists for this transport/path pair, or the cached one is dead.
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
    let key = (endpoint.transport, endpoint.path.clone());

    // ── Fast path: check the cache without blocking on connect ──
    {
        let guard = recover_lock(&MANAGER);
        if let Some(conn) = guard.get(&key) {
            if conn.is_alive() {
                return Ok(conn.clone());
            }
        }
    }

    // Only one caller may perform the blocking subscribe handshake. Other
    // callers wait here, then reuse the connection published by the winner.
    let _connect_guard = recover_lock(&CONNECT_GATE);

    // A caller may have populated the cache while we waited for the gate.
    {
        let guard = recover_lock(&MANAGER);
        if let Some(conn) = guard.get(&key) {
            if conn.is_alive() {
                return Ok(conn.clone());
            }
        }
    }

    let conn = MultiplexedConnection::connect(endpoint, connect_timeout)?;

    // Publish while the connect gate is still held, so no concurrent cold
    // caller can create a second reader/subscription.
    let mut guard = recover_lock(&MANAGER);
    guard.retain(|_, cached| cached.is_alive());
    guard.insert(key, conn.clone());
    Ok(conn)
}

/// Retire all cached connections. Called when the kernel is stopped so
/// the next request reconnects cleanly instead of reusing a dead handle.
pub fn reset() {
    // Wait for an in-flight connect to finish before clearing the cache. This
    // prevents an old-endpoint connection from being published after reset.
    let _connect_guard = recover_lock(&CONNECT_GATE);
    let mut guard = recover_lock(&MANAGER);
    for (_, conn) in guard.drain() {
        conn.retire();
    }
}

pub fn reset_endpoint(endpoint: &CoreEndpoint) {
    let _connect_guard = recover_lock(&CONNECT_GATE);
    let mut guard = recover_lock(&MANAGER);
    if let Some(conn) = guard.remove(&(endpoint.transport, endpoint.path.clone())) {
        conn.retire();
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
    if conn.received_within(activity_window) || conn.has_pending_requests() {
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
        assert_eq!(error.code, "permission_denied");
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
