//! Platform transport layer for kernel IPC.
//!
//! Raw socket (Unix) / named-pipe (Windows) I/O with JSON-line framing.
//!
//! Two connection modes are supported:
//! - **Single-shot** ([`send_json_line_request`]): opens a fresh connection
//!   per call. The kernel closes non-subscribe connections after responding,
//!   so this is only suitable for one-off probes.
//! - **Split / multiplexed** ([`connect_split`] → [`KernelReader`] +
//!   [`KernelWriter`]): opens one connection and returns independent
//!   read/write halves so a long-lived connection can run a background reader
//!   while the request path writes frames. This is what
//!   [`crate::kernel::connection`] uses to multiplex requests and events over
//!   a single subscribe connection.
//!
//! This module is kernel-agnostic — any kernel using JSON-line IPC over
//! domain sockets or named pipes can reuse it unchanged.

use serde_json::Value;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::time::Duration;

use crate::errors::{AppError, AppResult};
use crate::models::core::CoreEndpoint;

#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::mem;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use std::sync::Arc;
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, ERROR_IO_PENDING, GENERIC_READ, GENERIC_WRITE, HANDLE,
        INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Storage::FileSystem::{CreateFileW, ReadFile, WriteFile, FILE_FLAG_OVERLAPPED, OPEN_EXISTING},
    System::{
        Pipes::WaitNamedPipeW,
        Threading::{CreateEventW, WaitForSingleObject},
        IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
    },
};

pub(crate) trait ReadWrite: Read + Write + Send {}

impl<T> ReadWrite for T where T: Read + Write + Send {}

// ── Public API ──────────────────────────────────────────────────────

pub struct EventStream {
    endpoint: CoreEndpoint,
    reader: BufReader<Box<dyn ReadWrite>>,
}

/// Return the platform-specific default endpoint for the given kernel name.
///
/// On Windows, uses a well known named pipe: `\\.\pipe\zero-control`.
/// On Unix, uses the Zero daemon default: `~/.zero/control.sock`.
pub fn default_endpoint(kernel_name: &str) -> AppResult<CoreEndpoint> {
    Ok(CoreEndpoint {
        transport: transport_name(),
        path: default_socket_path(kernel_name)?,
    })
}

/// Connect to `endpoint`, send a single JSON-line `frame`, read one JSON response.
///
/// Opens a fresh connection per call. Only suitable for one-off probes — the
/// kernel closes non-subscribe connections after responding, so this cannot
/// be used for request multiplexing. For the multiplexed path see
/// [`crate::kernel::connection`].
pub fn send_json_line_request(
    endpoint: CoreEndpoint,
    frame: Vec<u8>,
    timeout: Duration,
) -> AppResult<Value> {
    let mut stream = connect(&endpoint, StreamTimeouts::request(timeout))?;
    stream
        .write_all(&frame)
        .map_err(|error| AppError::from_io("failed to write IPC request", &endpoint, error))?;
    stream
        .flush()
        .map_err(|error| AppError::from_io("failed to flush IPC request", &endpoint, error))?;

    let mut reader = BufReader::new(stream);
    read_json_line(&mut reader, &endpoint)
}

/// Connect to `endpoint`, send a subscribe `frame`, return a persistent `EventStream`.
///
/// Retained for one-off subscribers; the multiplexed path in
/// [`crate::kernel::connection`] supersedes this for normal operation.
#[allow(dead_code)]
pub fn subscribe(
    endpoint: CoreEndpoint,
    frame: Vec<u8>,
    timeout: Duration,
) -> AppResult<EventStream> {
    let mut stream = connect(&endpoint, StreamTimeouts::event_stream(timeout))?;
    stream.write_all(&frame).map_err(|error| {
        AppError::from_io("failed to write IPC subscribe request", &endpoint, error)
    })?;
    stream.flush().map_err(|error| {
        AppError::from_io("failed to flush IPC subscribe request", &endpoint, error)
    })?;

    Ok(EventStream {
        endpoint,
        reader: BufReader::new(stream),
    })
}

/// Serialize a `Value` into a JSON-line frame (trailing `\n`).
pub fn serialize_frame(frame: &Value) -> AppResult<Vec<u8>> {
    if !frame.is_object() {
        return Err(AppError::invalid_argument(
            "IPC frame must be a JSON object",
        ));
    }

    let mut bytes = serde_json::to_vec(frame).map_err(|error| AppError {
        code: "invalid_argument",
        message: format!("failed to serialize IPC frame: {error}"),
        details: None,
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

// ── EventStream ─────────────────────────────────────────────────────

impl EventStream {
    /// Read the next JSON-line frame (skips SSE comments and blank lines).
    #[allow(dead_code)]
    pub fn read_next(&mut self) -> AppResult<Value> {
        read_json_line(&mut self.reader, &self.endpoint)
    }
}

// ── Platform helpers ────────────────────────────────────────────────

pub fn transport_name() -> &'static str {
    if cfg!(windows) {
        "named-pipe"
    } else {
        "unix-socket"
    }
}

#[cfg(windows)]
fn default_socket_path(kernel_name: &str) -> AppResult<String> {
    Ok(format!(r"\\.\pipe\{kernel_name}-control"))
}

#[cfg(unix)]
fn default_socket_path(_kernel_name: &str) -> AppResult<String> {
    // Default Zero daemon socket: ~/.zero/control.sock
    let home = dirs::home_dir().ok_or_else(|| AppError {
        code: "internal",
        message: "cannot determine home directory for default socket path".to_string(),
        details: None,
    })?;
    Ok(home
        .join(".zero")
        .join("control.sock")
        .to_string_lossy()
        .to_string())
}

/// Compute a GUI-managed socket path relative to the kernel executable.
/// Used when spawning a managed kernel with `--control-socket` override.
#[cfg(unix)]
pub fn default_socket_path_for_executable(
    executable: Option<&std::path::Path>,
    kernel_name: &str,
) -> String {
    executable
        .and_then(std::path::Path::parent)
        .map(|parent| parent.join(format!("{kernel_name}-control.sock")))
        .unwrap_or_else(|| std::path::PathBuf::from(format!("{kernel_name}-control.sock")))
        .to_string_lossy()
        .to_string()
}

// ── JSON-line framing ───────────────────────────────────────────────

/// Read one JSON object terminated by `\n`. SSE-style lines starting with
/// `:` (heartbeat) and blank lines are skipped silently.
pub(crate) fn read_json_line<R: Read>(
    reader: &mut BufReader<R>,
    endpoint: &CoreEndpoint,
) -> AppResult<Value> {
    let mut line = String::new();

    loop {
        line.clear();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|error| AppError::from_io("failed to read IPC response", endpoint, error))?;

        if bytes == 0 {
            return Err(AppError::connection_closed(endpoint));
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() || trimmed.starts_with(':') {
            continue;
        }

        return serde_json::from_str(trimmed).map_err(|error| AppError {
            code: "invalid_response",
            message: format!("core IPC returned invalid JSON: {error}"),
            details: Some(serde_json::json!({
                "endpoint": endpoint.path,
                "line": trimmed,
            })),
        });
    }
}

// ── Connection timeouts ─────────────────────────────────────────────

struct StreamTimeouts {
    read: Option<Duration>,
    write: Option<Duration>,
}

impl StreamTimeouts {
    fn request(timeout: Duration) -> Self {
        Self {
            read: Some(timeout),
            write: Some(timeout),
        }
    }

    fn event_stream(timeout: Duration) -> Self {
        Self {
            read: None,
            write: Some(timeout),
        }
    }
}

// ── Single-shot connection (probes) ─────────────────────────────────

#[allow(dead_code)]
pub(crate) fn connect_raw(
    endpoint: &CoreEndpoint,
    timeout: Duration,
) -> AppResult<Box<dyn ReadWrite>> {
    connect_platform(endpoint, StreamTimeouts::event_stream(timeout))
}

fn connect(endpoint: &CoreEndpoint, timeouts: StreamTimeouts) -> AppResult<Box<dyn ReadWrite>> {
    connect_platform(endpoint, timeouts)
}

// ── Split connection (multiplexed IPC) ──────────────────────────────
//
// `connect_split` opens one connection and returns independent reader and
// writer halves. The halves share the underlying handle on Windows (named
// pipes support concurrent overlapped I/O on a single handle) and are cloned
// file descriptors on Unix. This lets a background reader task drain frames
// while the request path writes — the foundation of request multiplexing.

/// Read half of a split kernel connection.
#[cfg(unix)]
pub struct KernelReader(UnixStream);
/// Write half of a split kernel connection.
#[cfg(unix)]
pub struct KernelWriter(UnixStream);

/// Connect once and return `(reader, writer)` halves for a multiplexed
/// connection. The read half has no timeout (the reader task blocks waiting
/// for frames); the write half inherits `timeout`.
pub fn connect_split(
    endpoint: &CoreEndpoint,
    timeout: Duration,
) -> AppResult<(KernelReader, KernelWriter)> {
    connect_split_platform(endpoint, StreamTimeouts::event_stream(timeout))
}

#[cfg(unix)]
impl Read for KernelReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

#[cfg(unix)]
impl Write for KernelWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

// ── Unix transport ──────────────────────────────────────────────────

#[cfg(unix)]
fn connect_platform(
    endpoint: &CoreEndpoint,
    timeouts: StreamTimeouts,
) -> AppResult<Box<dyn ReadWrite>> {
    let stream = UnixStream::connect(&endpoint.path)
        .map_err(|error| AppError::from_io("failed to connect to core IPC", endpoint, error))?;
    stream
        .set_read_timeout(timeouts.read)
        .map_err(|error| AppError::from_io("failed to set IPC read timeout", endpoint, error))?;
    stream
        .set_write_timeout(timeouts.write)
        .map_err(|error| AppError::from_io("failed to set IPC write timeout", endpoint, error))?;

    Ok(Box::new(stream))
}

#[cfg(unix)]
fn connect_split_platform(
    endpoint: &CoreEndpoint,
    timeouts: StreamTimeouts,
) -> AppResult<(KernelReader, KernelWriter)> {
    let reader = UnixStream::connect(&endpoint.path)
        .map_err(|error| AppError::from_io("failed to connect to core IPC", endpoint, error))?;
    reader
        .set_read_timeout(timeouts.read)
        .map_err(|error| AppError::from_io("failed to set IPC read timeout", endpoint, error))?;
    reader
        .set_write_timeout(timeouts.write)
        .map_err(|error| AppError::from_io("failed to set IPC write timeout", endpoint, error))?;
    // Duplicate the fd so the reader and writer can live on independent
    // threads without sharing a single owner.
    let writer = reader.try_clone().map_err(|error| {
        AppError::from_io(
            "failed to clone IPC stream for writer half",
            endpoint,
            error,
        )
    })?;
    Ok((KernelReader(reader), KernelWriter(writer)))
}

// ── Windows transport (overlapped named pipe) ───────────────────────

#[cfg(windows)]
fn connect_platform(
    endpoint: &CoreEndpoint,
    timeouts: StreamTimeouts,
) -> AppResult<Box<dyn ReadWrite>> {
    let pipe = NamedPipeClient::connect(&endpoint.path, timeouts)
        .map_err(|error| AppError::from_io("failed to connect to core IPC", endpoint, error))?;

    Ok(Box::new(pipe))
}

/// Single-owner named pipe client used by the single-shot connect path.
///
/// Thin wrapper over an [`SharedPipe`] (reference-counted so the same handle
/// can also back the split [`KernelReader`] / [`KernelWriter`] halves).
#[cfg(windows)]
struct NamedPipeClient(Arc<SharedPipe>);

#[cfg(windows)]
impl NamedPipeClient {
    fn connect(path: &str, timeouts: StreamTimeouts) -> io::Result<Self> {
        Ok(Self(Arc::new(SharedPipe::connect(path, timeouts)?)))
    }
}

#[cfg(windows)]
impl Read for NamedPipeClient {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read_overlapped(buffer)
    }
}

#[cfg(windows)]
impl Write for NamedPipeClient {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.write_overlapped(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Underlying named pipe handle, reference-counted so it can be shared
/// between a reader half and a writer half.
///
/// Windows permits multiple overlapped operations in flight on a single pipe
/// handle simultaneously (each uses its own `OVERLAPPED` + event), so a
/// reader blocked in `ReadFile` does not block a concurrent `WriteFile`.
#[cfg(windows)]
struct SharedPipe {
    handle: HANDLE,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
}

#[cfg(windows)]
unsafe impl Send for SharedPipe {}
#[cfg(windows)]
unsafe impl Sync for SharedPipe {}

#[cfg(windows)]
impl SharedPipe {
    /// Connect to a named pipe with a bounded timeout.
    ///
    /// `WaitNamedPipeW` distinguishes two failure modes by error code:
    /// - `ERROR_FILE_NOT_FOUND` (2):  no pipe server exists → fail fast
    /// - `ERROR_SEM_TIMEOUT` (121):   pipe exists but no free instance
    ///   within the wait interval → proceed to `CreateFileW` which may
    ///   succeed if a slot opened, or return `ERROR_PIPE_BUSY` (231).
    ///
    /// We must NOT bail on 121 — that would break connections when the
    /// kernel is running but briefly has no free pipe instances.
    fn connect(path: &str, timeouts: StreamTimeouts) -> io::Result<Self> {
        let path_wide = wide_null(path);
        let write_timeout_ms = timeouts.write.map(duration_to_millis).unwrap_or(2_000);

        let waited = unsafe { WaitNamedPipeW(path_wide.as_ptr(), write_timeout_ms) };

        // WaitNamedPipeW returns 0 on failure — check the specific error.
        // Only ERROR_FILE_NOT_FOUND means "pipe doesn't exist at all";
        // anything else (esp. ERROR_SEM_TIMEOUT) means the pipe server is
        // running but we should still try CreateFileW.
        if waited == 0 {
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(2) {
                // ERROR_FILE_NOT_FOUND — no pipe server, fail fast to
                // avoid CreateFileW blocking for the OS-default timeout.
                return Err(err);
            }
            // Other errors (121 = timeout, etc.) — pipe may still be
            // available, fall through to CreateFileW.
        }

        const MAX_RETRIES: u32 = 3;
        for retry in 0..MAX_RETRIES {
            let handle = unsafe {
                CreateFileW(
                    path_wide.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED,
                    std::ptr::null_mut(),
                )
            };

            if handle != INVALID_HANDLE_VALUE {
                return Ok(Self {
                    handle,
                    read_timeout: timeouts.read,
                    write_timeout: timeouts.write,
                });
            }

            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(231) && retry + 1 < MAX_RETRIES {
                // ERROR_PIPE_BUSY — all instances in use, brief backoff
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            return Err(err);
        }

        Err(io::Error::last_os_error())
    }

    fn read_overlapped(&self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }

        let mut bytes_read = 0_u32;
        let mut operation = OverlappedOperation::new()?;
        let started = unsafe {
            ReadFile(
                self.handle,
                buffer.as_mut_ptr(),
                buffer.len().min(u32::MAX as usize) as u32,
                std::ptr::addr_of_mut!(bytes_read),
                operation.overlapped_mut(),
            )
        };

        if started != 0 {
            return Ok(bytes_read as usize);
        }

        self.finish_overlapped(operation.overlapped_mut(), self.read_timeout)
    }

    fn write_overlapped(&self, buffer: &[u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }

        let mut bytes_written = 0_u32;
        let mut operation = OverlappedOperation::new()?;
        let started = unsafe {
            WriteFile(
                self.handle,
                buffer.as_ptr(),
                buffer.len().min(u32::MAX as usize) as u32,
                std::ptr::addr_of_mut!(bytes_written),
                operation.overlapped_mut(),
            )
        };

        if started != 0 {
            return Ok(bytes_written as usize);
        }

        self.finish_overlapped(operation.overlapped_mut(), self.write_timeout)
    }

    fn finish_overlapped(
        &self,
        overlapped: *mut OVERLAPPED,
        timeout: Option<Duration>,
    ) -> io::Result<usize> {
        let error = unsafe { GetLastError() };
        if error != ERROR_IO_PENDING {
            return Err(io::Error::from_raw_os_error(error as i32));
        }

        if let Some(timeout) = timeout {
            let wait_result =
                unsafe { WaitForSingleObject((*overlapped).hEvent, duration_to_millis(timeout)) };
            if wait_result == WAIT_TIMEOUT {
                unsafe {
                    CancelIoEx(self.handle, overlapped);
                }
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "named pipe operation timed out",
                ));
            }
            if wait_result != WAIT_OBJECT_0 {
                return Err(io::Error::last_os_error());
            }
        }

        let mut transferred = 0_u32;
        let result =
            unsafe { GetOverlappedResult(self.handle, overlapped, &mut transferred, true.into()) };
        if result == 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(transferred as usize)
    }
}

#[cfg(windows)]
impl Drop for SharedPipe {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

// ── Windows split halves ────────────────────────────────────────────

#[cfg(windows)]
pub struct KernelReader {
    shared: Arc<SharedPipe>,
}

#[cfg(windows)]
pub struct KernelWriter {
    shared: Arc<SharedPipe>,
}

#[cfg(windows)]
impl Read for KernelReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.shared.read_overlapped(buffer)
    }
}

#[cfg(windows)]
impl Write for KernelWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.shared.write_overlapped(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
fn connect_split_platform(
    endpoint: &CoreEndpoint,
    timeouts: StreamTimeouts,
) -> AppResult<(KernelReader, KernelWriter)> {
    let shared = Arc::new(
        SharedPipe::connect(&endpoint.path, timeouts)
            .map_err(|error| AppError::from_io("failed to connect to core IPC", endpoint, error))?,
    );
    Ok((
        KernelReader {
            shared: Arc::clone(&shared),
        },
        KernelWriter { shared },
    ))
}

// ── Windows overlapped helpers ──────────────────────────────────────

#[cfg(windows)]
struct OverlappedOperation {
    overlapped: OVERLAPPED,
}

#[cfg(windows)]
impl OverlappedOperation {
    fn new() -> io::Result<Self> {
        let event = unsafe {
            CreateEventW(
                std::ptr::null(),
                true.into(),
                false.into(),
                std::ptr::null(),
            )
        };
        if event.is_null() {
            return Err(io::Error::last_os_error());
        }

        let mut overlapped = unsafe { mem::zeroed::<OVERLAPPED>() };
        overlapped.hEvent = event;

        Ok(Self { overlapped })
    }

    fn overlapped_mut(&mut self) -> *mut OVERLAPPED {
        &mut self.overlapped
    }
}

#[cfg(windows)]
impl Drop for OverlappedOperation {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.overlapped.hEvent);
        }
    }
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

#[cfg(windows)]
fn duration_to_millis(duration: Duration) -> u32 {
    duration.as_millis().min(u32::MAX as u128) as u32
}
