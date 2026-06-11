//! Single persistent IPC connection with request/event multiplexing.
//!
//! One pipe connection → one OS object → two application-layer handles
//! (reader + writer) via Arc<UnsafeCell<...>>.  Both share the same
//! underlying OS handle; the kernel sees exactly one client.
//!
//! The OS natively supports concurrent read/write on pipes and sockets.
//! Rust's `&mut` requirement is bypassed via `UnsafeCell` — safe because
//! we never hold both `&mut Read` and `&mut Write` to the same byte range,
//! and the OS serialises pipe I/O internally.

use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::oneshot;

use crate::errors::{AppError, AppResult};
use crate::models::core::CoreEndpoint;

use super::transport;

type PendingRequests = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, AppError>>>>>;
type EventSender = tokio::sync::broadcast::Sender<Value>;

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// A read or write view over a shared OS pipe handle.
struct SharedStream(Arc<UnsafeCell<Box<dyn transport::ReadWrite>>>);

// SAFETY: OS pipes/sockets handle concurrent read/write natively.
// `Writer` serialises writes via a separate Mutex.  `Reader` has
// exclusive read access via its dedicated SharedStream instance.
unsafe impl Send for SharedStream {}
unsafe impl Sync for SharedStream {}

impl Read for SharedStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        unsafe { &mut *self.0.get() }.read(buf)
    }
}

impl Write for SharedStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        unsafe { &mut *self.0.get() }.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        unsafe { &mut *self.0.get() }.flush()
    }
}

pub(crate) struct MultiplexedIpc {
    /// Dedicated reader, owned by the background thread.
    reader: Mutex<Option<SharedStream>>,
    /// Serialised writer (Mutex prevents interleaved frames).
    writer: Mutex<SharedStream>,
    pending: PendingRequests,
    events: EventSender,
    endpoint: CoreEndpoint,
    subscribed: Mutex<bool>,
}

impl MultiplexedIpc {
    pub fn connect(endpoint: CoreEndpoint, timeout: Duration) -> AppResult<Arc<Self>> {
        let stream = transport::connect_raw(&endpoint, timeout)?;
        let shared = Arc::new(UnsafeCell::new(stream));
        let reader = SharedStream(Arc::clone(&shared));
        let writer = SharedStream(shared);

        let pending: PendingRequests = Arc::new(Mutex::new(HashMap::new()));
        let events: EventSender = tokio::sync::broadcast::channel(256).0;

        let conn = Arc::new(Self {
            reader: Mutex::new(Some(reader)),
            writer: Mutex::new(writer),
            pending: pending.clone(),
            events: events.clone(),
            endpoint: endpoint.clone(),
            subscribed: Mutex::new(false),
        });

        let conn_reader = Arc::clone(&conn);
        let ep = endpoint.clone();
        std::thread::spawn(move || conn_reader.read_loop(ep, pending, events));

        Ok(conn)
    }

    pub fn send_request(self: &Arc<Self>, frame: Value, timeout: Duration) -> AppResult<Value> {
        let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let mut frame = frame;
        if let Some(obj) = frame.as_object_mut() {
            if !obj.contains_key("id") {
                obj.insert("id".to_string(), Value::Number(id.into()));
            }
        }

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().map_err(|_| AppError::internal("pending lock poisoned"))?;
            pending.insert(id, tx);
        }

        {
            let frame_bytes = transport::serialize_frame(&frame)?;
            let mut writer = self.writer.lock().map_err(|_| AppError::internal("writer lock poisoned"))?;
            writer.write_all(&frame_bytes).map_err(|e| AppError::from_io("failed to write IPC frame", &self.endpoint, e))?;
            writer.flush().map_err(|e| AppError::from_io("failed to flush IPC frame", &self.endpoint, e))?;
        }

        match tokio::runtime::Handle::try_current() {
            Ok(handle) => match handle.block_on(tokio::time::timeout(timeout, rx)) {
                Ok(Ok(response)) => response,
                Ok(Err(_)) => { let _ = self.pending.lock().map(|mut p| p.remove(&id)); Err(AppError::internal("IPC connection closed")) }
                Err(_) => { let _ = self.pending.lock().map(|mut p| p.remove(&id)); Err(AppError::internal(format!("IPC request timed out after {}ms", timeout.as_millis()))) }
            },
            Err(_) => match rx.blocking_recv() {
                Ok(response) => response,
                Err(_) => { let _ = self.pending.lock().map(|mut p| p.remove(&id)); Err(AppError::internal("IPC connection closed")) }
            },
        }
    }

    pub fn subscribe(self: &Arc<Self>, event_types: Option<&[String]>) -> AppResult<tokio::sync::broadcast::Receiver<Value>> {
        let mut subscribed = self.subscribed.lock().map_err(|_| AppError::internal("subscribed lock poisoned"))?;
        if *subscribed { return Ok(self.events.subscribe()); }
        let frame = match event_types {
            Some(types) => serde_json::json!({ "type": "subscribe", "events": types }),
            None => serde_json::json!({ "type": "subscribe" }),
        };
        let confirmation = self.send_request(frame, Duration::from_secs(5))?;
        let ok = confirmation.as_object().and_then(|obj| obj.get("ok")).and_then(|v| v.as_bool()).unwrap_or(false);
        if !ok { return Err(AppError::core_response(confirmation)); }
        *subscribed = true;
        Ok(self.events.subscribe())
    }

    fn read_loop(&self, endpoint: CoreEndpoint, pending: PendingRequests, events: EventSender) {
        let mut reader = self.reader.lock().ok().and_then(|mut g| g.take()).expect("reader already consumed");
        let mut buf = String::new();
        loop {
            buf.clear();
            match Self::read_json_line(&mut reader, &mut buf, &endpoint) {
                Ok(value) => Self::route_frame(value, &pending, &events),
                Err(_) => break,
            }
        }
    }

    fn route_frame(value: Value, pending: &PendingRequests, events: &EventSender) {
        if value.as_object().map_or(false, |obj| obj.contains_key("ok")) {
            let id = value.as_object().and_then(|obj| obj.get("id")).and_then(|v| v.as_u64());
            let response: Result<Value, AppError> = if value.as_object().and_then(|obj| obj.get("ok")).and_then(|v| v.as_bool()) == Some(true) {
                Ok(value)
            } else {
                Err(AppError::core_response(value))
            };
            if let Some(id) = id {
                if let Ok(mut pending) = pending.lock() {
                    if let Some(tx) = pending.remove(&id) { let _ = tx.send(response); }
                }
            }
        } else if value.as_object().map_or(false, |obj| obj.contains_key("schema_id")) {
            let _ = events.send(value);
        }
    }

    fn read_json_line(reader: &mut dyn Read, buf: &mut String, endpoint: &CoreEndpoint) -> AppResult<Value> {
        loop {
            buf.clear();
            let mut byte_buf = [0u8; 1];
            loop {
                match reader.read(&mut byte_buf) {
                    Ok(0) => return Err(AppError::connection_closed(endpoint)),
                    Ok(_) => { if byte_buf[0] == b'\n' { break; } buf.push(byte_buf[0] as char); }
                    Err(e) => return Err(AppError::from_io("failed to read IPC frame", endpoint, e)),
                }
            }
            let line = buf.trim();
            if line.is_empty() || line.starts_with(':') { continue; }
            return serde_json::from_str::<Value>(line).map_err(|error| AppError {
                code: "internal", message: format!("failed to parse IPC frame: {error}"),
                details: Some(serde_json::json!({ "raw": line })),
            });
        }
    }
}

// ── Global singleton ─────────────────────────────────────────────

use std::sync::OnceLock;
static GLOBAL_IPC: OnceLock<Mutex<Option<Arc<MultiplexedIpc>>>> = OnceLock::new();

pub(crate) fn get_or_connect(endpoint: CoreEndpoint, timeout: Duration) -> AppResult<Arc<MultiplexedIpc>> {
    let cell = GLOBAL_IPC.get_or_init(|| Mutex::new(None));
    let mut guard = cell.lock().map_err(|_| AppError::internal("global ipc lock poisoned"))?;
    if let Some(ref conn) = *guard { return Ok(Arc::clone(conn)); }
    let conn = MultiplexedIpc::connect(endpoint, timeout)?;
    *guard = Some(Arc::clone(&conn));
    Ok(conn)
}

pub(crate) fn subscribe_events(endpoint: CoreEndpoint, event_types: Option<&[String]>) -> AppResult<tokio::sync::broadcast::Receiver<Value>> {
    get_or_connect(endpoint, Duration::from_secs(5))?.subscribe(event_types)
}
