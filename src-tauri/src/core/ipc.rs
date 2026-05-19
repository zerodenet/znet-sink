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

trait ReadWrite: Read + Write + Send {}

impl<T> ReadWrite for T where T: Read + Write + Send {}

pub(crate) struct EventStream {
    endpoint: CoreEndpoint,
    reader: BufReader<Box<dyn ReadWrite>>,
}

pub(crate) fn default_endpoint() -> AppResult<CoreEndpoint> {
    Ok(CoreEndpoint {
        transport: transport_name(),
        path: default_socket_path()?,
    })
}

pub(crate) fn send_json_line_request(
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

pub(crate) fn subscribe(
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

impl EventStream {
    pub(crate) fn read_next(&mut self) -> AppResult<Value> {
        read_json_line(&mut self.reader, &self.endpoint)
    }
}

pub(crate) fn serialize_frame(frame: &Value) -> AppResult<Vec<u8>> {
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

fn read_json_line(
    reader: &mut BufReader<Box<dyn ReadWrite>>,
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

#[cfg(windows)]
pub(crate) fn default_socket_path() -> AppResult<String> {
    Ok(r"\\.\pipe\zero-control".to_string())
}

#[cfg(unix)]
pub(crate) fn default_socket_path() -> AppResult<String> {
    Ok(default_socket_path_for_executable(
        std::env::current_exe().ok().as_deref(),
    ))
}

#[cfg(unix)]
pub(crate) fn default_socket_path_for_executable(executable: Option<&std::path::Path>) -> String {
    executable
        .and_then(std::path::Path::parent)
        .map(|parent| parent.join("zero-control.sock"))
        .unwrap_or_else(|| std::path::PathBuf::from("zero-control.sock"))
        .to_string_lossy()
        .to_string()
}

#[cfg(windows)]
pub(crate) fn transport_name() -> &'static str {
    "named-pipe"
}

#[cfg(unix)]
pub(crate) fn transport_name() -> &'static str {
    "unix-socket"
}

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

fn connect(endpoint: &CoreEndpoint, timeouts: StreamTimeouts) -> AppResult<Box<dyn ReadWrite>> {
    connect_platform(endpoint, timeouts)
}

#[cfg(windows)]
fn connect_platform(
    endpoint: &CoreEndpoint,
    timeouts: StreamTimeouts,
) -> AppResult<Box<dyn ReadWrite>> {
    let pipe = NamedPipeClient::connect(&endpoint.path, timeouts)
        .map_err(|error| AppError::from_io("failed to connect to core IPC", endpoint, error))?;

    Ok(Box::new(pipe))
}

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

#[cfg(windows)]
struct NamedPipeClient {
    handle: HANDLE,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
}

// The pipe handle is owned by this client and all operations happen through &mut self on
// Tauri's blocking worker thread.
#[cfg(windows)]
unsafe impl Send for NamedPipeClient {}

#[cfg(windows)]
impl NamedPipeClient {
    fn connect(path: &str, timeouts: StreamTimeouts) -> io::Result<Self> {
        let path_wide = wide_null(path);
        if let Some(write_timeout) = timeouts.write {
            let _ =
                unsafe { WaitNamedPipeW(path_wide.as_ptr(), duration_to_millis(write_timeout)) };
        }

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

        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        Ok(Self {
            handle,
            read_timeout: timeouts.read,
            write_timeout: timeouts.write,
        })
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

        self.finish_overlapped(started, operation.overlapped_mut(), self.read_timeout)
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

        self.finish_overlapped(started, operation.overlapped_mut(), self.write_timeout)
    }

    fn finish_overlapped(
        &self,
        started: i32,
        overlapped: *mut OVERLAPPED,
        timeout: Option<Duration>,
    ) -> io::Result<usize> {
        if started == 0 {
            let error = unsafe { GetLastError() };
            if error != ERROR_IO_PENDING {
                return Err(io::Error::from_raw_os_error(error as i32));
            }
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
impl Read for NamedPipeClient {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.read_overlapped(buffer)
    }
}

#[cfg(windows)]
impl Write for NamedPipeClient {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.write_overlapped(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for NamedPipeClient {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

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
