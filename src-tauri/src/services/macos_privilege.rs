//! Process-lifetime macOS authorization helper.
//!
//! The GUI stays unprivileged. On the first proxy or host DNS mutation it starts
//! one narrowly-scoped root helper through the native macOS authorization
//! dialog, then reuses that helper until the GUI exits. The helper accepts
//! fixed `networksetup` proxy mutations and validated, temporary TUN DNS sessions.

mod launch;
mod tun_dns;

use launch::launch_authorized_helper;

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

const HELPER_COMMAND: &str = "__macos-networksetup-helper";

static HELPER_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static PRIVILEGED_HELPER: OnceLock<Mutex<Option<UnixStream>>> = OnceLock::new();
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

/// Quit may reuse existing authorization, but must never ask for new authority.
pub(crate) fn begin_shutdown() {
    SHUTTING_DOWN.store(true, Ordering::SeqCst);
}

fn require_interactive_authorization(allowed: bool) -> AppResult<()> {
    if allowed {
        Ok(())
    } else {
        Err(AppError::internal(
            "network cleanup requires administrator authorization; no prompt is allowed during shutdown; retained proxy ownership for recovery",
        ))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum HelperRequest {
    NetworkSetup {
        commands: Vec<Vec<String>>,
    },
    TunDns {
        interface: Option<String>,
        server: Option<String>,
        pid: u32,
    },
}

#[derive(Deserialize, Serialize)]
struct HelperResponse {
    success: bool,
    code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

struct HelperSocketGuard {
    directory: PathBuf,
    socket: PathBuf,
}

impl Drop for HelperSocketGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket);
        let _ = fs::remove_dir(&self.directory);
    }
}

/// Intercept the private helper command before Tauri starts.
pub fn run_if_requested() -> Option<io::Result<()>> {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new(HELPER_COMMAND)) {
        return None;
    }
    let Some(flag) = arguments.next() else {
        return Some(Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "macOS networksetup helper requires --socket PATH",
        )));
    };
    let Some(socket) = arguments.next() else {
        return Some(Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "macOS networksetup helper requires --socket PATH",
        )));
    };
    if flag != "--socket" || arguments.next().is_some() {
        return Some(Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid macOS networksetup helper arguments",
        )));
    }
    Some(run_helper(Path::new(&socket)))
}

pub(crate) fn run_networksetup_commands(commands: &[Vec<String>]) -> AppResult<()> {
    if commands.is_empty() {
        return Ok(());
    }
    for command in commands {
        validate_networksetup_command(command).map_err(|error| {
            AppError::internal(format!("refusing unsafe networksetup request: {error}"))
        })?;
    }

    let mut slot = helper_slot()
        .lock()
        .map_err(|_| AppError::internal("macOS networksetup helper lock is poisoned"))?;
    execute_with_helper(
        &mut slot,
        || !SHUTTING_DOWN.load(Ordering::SeqCst),
        launch_authorized_helper,
        commands,
    )
}

fn execute_with_helper(
    slot: &mut Option<UnixStream>,
    may_authorize: impl Fn() -> bool,
    launch: impl FnOnce() -> AppResult<UnixStream>,
    commands: &[Vec<String>],
) -> AppResult<()> {
    if let Some(stream) = slot.as_mut() {
        match request(stream, commands) {
            Ok(response) => return response_result(response),
            Err(error) if helper_connection_lost(&error) => {
                slot.take();
            }
            Err(error) => {
                return Err(AppError::internal(format!(
                    "communicate with authorized networksetup helper: {error}"
                )))
            }
        }
    }

    require_interactive_authorization(may_authorize())?;
    let mut stream = launch()?;
    let response = request(&mut stream, commands).map_err(|error| {
        AppError::internal(format!(
            "communicate with authorized networksetup helper: {error}"
        ))
    })?;
    *slot = Some(stream);
    response_result(response)
}

pub(crate) fn set_tun_dns(
    interface: Option<&str>,
    server: Option<&str>,
    pid: u32,
) -> AppResult<()> {
    update_tun_dns(interface, server, pid, true)
}

pub(crate) fn refresh_tun_dns(interface: &str, server: &str, pid: u32) -> AppResult<()> {
    update_tun_dns(Some(interface), Some(server), pid, false)
}

fn dns_helper(
    slot: &mut Option<UnixStream>,
    may_authorize: bool,
    launch: impl FnOnce() -> AppResult<UnixStream>,
) -> AppResult<&mut UnixStream> {
    if slot.is_none() {
        if !may_authorize {
            return Err(AppError::internal(
                "已授权的网络 helper 不可用，请手动重试；后台刷新不会请求新授权",
            ));
        }
        *slot = Some(launch()?);
    }
    Ok(slot.as_mut().unwrap())
}

fn update_tun_dns(
    interface: Option<&str>,
    server: Option<&str>,
    pid: u32,
    may_authorize: bool,
) -> AppResult<()> {
    let mut slot = helper_slot()
        .lock()
        .map_err(|_| AppError::internal("DNS helper lock poisoned"))?;
    if interface.is_none() && slot.is_none() {
        return Ok(());
    }
    let stream = dns_helper(&mut slot, may_authorize, launch_authorized_helper)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|error| AppError::internal(error.to_string()))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|error| AppError::internal(error.to_string()))?;
    let result = (|| -> io::Result<HelperResponse> {
        serde_json::to_writer(
            &mut *stream,
            &HelperRequest::TunDns {
                interface: interface.map(str::to_owned),
                server: server.map(str::to_owned),
                pid,
            },
        )
        .map_err(io::Error::from)?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        let mut line = String::new();
        BufReader::new(stream.try_clone()?).read_line(&mut line)?;
        serde_json::from_str(&line).map_err(io::Error::other)
    })();
    let _ = stream.set_read_timeout(None);
    let _ = stream.set_write_timeout(None);
    match result {
        Ok(response) => response_result(response),
        Err(error) => {
            slot.take();
            Err(AppError::internal(format!("TUN DNS helper: {error}")))
        }
    }
}

pub(crate) fn has_authorized_helper() -> bool {
    helper_slot()
        .lock()
        .map(|slot| slot.is_some())
        .unwrap_or(false)
}

fn request(stream: &mut UnixStream, commands: &[Vec<String>]) -> io::Result<HelperResponse> {
    serde_json::to_writer(
        &mut *stream,
        &HelperRequest::NetworkSetup {
            commands: commands.to_vec(),
        },
    )
    .map_err(io::Error::from)?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut line = String::new();
    BufReader::new(stream.try_clone()?).read_line(&mut line)?;
    if line.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "authorized networksetup helper closed the control channel",
        ));
    }
    serde_json::from_str(&line).map_err(io::Error::other)
}

fn response_result(response: HelperResponse) -> AppResult<()> {
    if response.success {
        return Ok(());
    }
    let detail = if response.stderr.is_empty() {
        String::from_utf8_lossy(&response.stdout)
    } else {
        String::from_utf8_lossy(&response.stderr)
    };
    Err(AppError::internal(format!(
        "administrator-authorized networksetup failed (status {}): {}",
        response.code,
        detail.trim()
    )))
}

fn run_helper(socket_path: &Path) -> io::Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "macOS networksetup helper must run with administrator privileges",
        ));
    }
    let mut stream = UnixStream::connect(socket_path)?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut dns: Option<tun_dns::Resolver> = None;
    loop {
        if dns
            .as_ref()
            .is_some_and(|guard| !tun_dns::Resolver::alive(&guard.interface, guard.pid))
        {
            dns.take();
        }
        let mut poll = libc::pollfd {
            fd: stream.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut poll, 1, 1000) };
        if ready == 0 {
            continue;
        }
        if ready < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        if ready < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let request: HelperRequest = serde_json::from_str(&line).map_err(io::Error::other)?;
        let response = match request {
            HelperRequest::NetworkSetup { commands } => run_networksetup_as_root(&commands),
            HelperRequest::TunDns {
                interface,
                server,
                pid,
            } => {
                let result = match (interface, server) {
                    (Some(interface), Some(server)) => {
                        if dns.as_ref().is_some_and(|guard| {
                            guard.interface == interface
                                && guard.server == server
                                && guard.pid == pid
                        }) {
                            dns.as_ref().unwrap().refresh()
                        } else {
                            tun_dns::Resolver::install(&interface, &server, pid).map(|guard| {
                                dns = Some(guard);
                            })
                        }
                    }
                    (None, None) => {
                        dns.take();
                        Ok(())
                    }
                    _ => Err(io::Error::other("invalid TUN DNS request")),
                };
                HelperResponse {
                    success: result.is_ok(),
                    code: if result.is_ok() { 0 } else { 1 },
                    stdout: Vec::new(),
                    stderr: result
                        .err()
                        .map(|error| error.to_string().into_bytes())
                        .unwrap_or_default(),
                }
            }
        };
        serde_json::to_writer(&mut stream, &response).map_err(io::Error::other)?;
        stream.write_all(b"\n")?;
        stream.flush()?;
    }
}

fn run_networksetup_as_root(commands: &[Vec<String>]) -> HelperResponse {
    for arguments in commands {
        if let Err(error) = validate_networksetup_command(arguments) {
            return HelperResponse {
                success: false,
                code: 2,
                stdout: Vec::new(),
                stderr: error.to_string().into_bytes(),
            };
        }
        match Command::new("/usr/sbin/networksetup")
            .args(arguments)
            .output()
        {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                return HelperResponse {
                    success: false,
                    code: output.status.code().unwrap_or(1),
                    stdout: output.stdout,
                    stderr: output.stderr,
                }
            }
            Err(error) => {
                return HelperResponse {
                    success: false,
                    code: error.raw_os_error().unwrap_or(1),
                    stdout: Vec::new(),
                    stderr: error.to_string().into_bytes(),
                }
            }
        }
    }
    HelperResponse {
        success: true,
        code: 0,
        stdout: Vec::new(),
        stderr: Vec::new(),
    }
}

fn validate_networksetup_command(arguments: &[String]) -> io::Result<()> {
    let Some(operation) = arguments.first().map(String::as_str) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "empty networksetup command",
        ));
    };
    let valid = match operation {
        "-setwebproxy" | "-setsecurewebproxy" | "-setsocksfirewallproxy" => arguments.len() == 4,
        "-setproxybypassdomains" => {
            arguments.len() >= 3
                && arguments
                    .iter()
                    .skip(1)
                    .all(|value| !value.is_empty() && !value.contains(['\0', '\n', '\r']))
        }
        "-setwebproxystate" | "-setsecurewebproxystate" | "-setsocksfirewallproxystate" => {
            arguments.len() == 3
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported networksetup mutation `{operation}`"),
        ))
    }
}

fn bind_helper_socket() -> io::Result<(UnixListener, HelperSocketGuard)> {
    for _ in 0..32 {
        let sequence = HELPER_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "znet-sink-networksetup-{}-{}-{sequence}",
            unsafe { libc::geteuid() },
            std::process::id()
        ));
        match fs::create_dir(&directory) {
            Ok(()) => {
                fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
                let socket = directory.join("helper.sock");
                let guard = HelperSocketGuard { directory, socket };
                let listener = UnixListener::bind(&guard.socket)?;
                fs::set_permissions(&guard.socket, fs::Permissions::from_mode(0o600))?;
                return Ok((listener, guard));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a private networksetup helper socket",
    ))
}

fn peer_effective_uid(stream: &UnixStream) -> io::Result<libc::uid_t> {
    let mut uid = 0;
    let mut gid = 0;
    if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(uid)
}

fn helper_slot() -> &'static Mutex<Option<UnixStream>> {
    PRIVILEGED_HELPER.get_or_init(|| Mutex::new(None))
}

fn helper_connection_lost(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::NotConnected
            | io::ErrorKind::UnexpectedEof
    )
}

fn authorization_failure(output: &Output) -> AppError {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    if diagnostics.contains("(-128)")
        || diagnostics.contains("user canceled")
        || diagnostics.contains("user cancelled")
        || diagnostics.contains("用户已取消")
    {
        return AppError::authorization_cancelled(
            "macOS administrator authorization was cancelled",
        );
    }
    let detail = if !stderr.trim().is_empty() {
        stderr.trim()
    } else if !stdout.trim().is_empty() {
        stdout.trim()
    } else {
        "no diagnostic output"
    };
    AppError::internal(format!(
        "administrator authorization failed (status {}): {detail}",
        output.status
    ))
}

#[cfg(test)]
#[path = "macos_privilege/tests.rs"]
mod tests;
