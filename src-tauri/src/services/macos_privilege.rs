//! Process-lifetime macOS authorization helper.
//!
//! The GUI stays unprivileged. On the first system-proxy mutation it starts
//! one narrowly-scoped root helper through the native macOS authorization
//! dialog, then reuses that helper until the GUI exits. The helper accepts
//! only the fixed `networksetup` mutations used by the system-proxy backend.

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

const HELPER_COMMAND: &str = "__macos-networksetup-helper";
const PRIVILEGED_COMMAND_SCRIPT: &str = r#"
on run argv
    if (count of argv) is 0 then error "missing privileged command"
    set commandText to quoted form of (item 1 of argv)
    repeat with argumentIndex from 2 to (count of argv)
        set commandText to commandText & " " & quoted form of (item argumentIndex of argv)
    end repeat
    do shell script commandText with administrator privileges
end run
"#;

static HELPER_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static PRIVILEGED_HELPER: OnceLock<Mutex<Option<UnixStream>>> = OnceLock::new();

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum HelperRequest {
    NetworkSetup { commands: Vec<Vec<String>> },
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

    let mut stream = launch_authorized_helper()?;
    let response = request(&mut stream, commands).map_err(|error| {
        AppError::internal(format!(
            "communicate with authorized networksetup helper: {error}"
        ))
    })?;
    *slot = Some(stream);
    response_result(response)
}

pub(crate) fn has_authorized_helper() -> bool {
    helper_slot()
        .lock()
        .map(|slot| slot.is_some())
        .unwrap_or(false)
}

fn launch_authorized_helper() -> AppResult<UnixStream> {
    let (listener, guard) = bind_helper_socket().map_err(|error| {
        AppError::internal(format!("prepare macOS networksetup helper: {error}"))
    })?;
    let executable = std::env::current_exe().map_err(|error| {
        AppError::internal(format!(
            "resolve ZNet Sink executable for authorization: {error}"
        ))
    })?;
    let executable = executable.to_string_lossy().into_owned();
    let socket = guard.socket.to_string_lossy().into_owned();
    let mut child = Command::new("/usr/bin/osascript")
        .args([
            "-e",
            PRIVILEGED_COMMAND_SCRIPT,
            "--",
            executable.as_str(),
            HELPER_COMMAND,
            "--socket",
            socket.as_str(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            AppError::internal(format!(
                "request administrator authorization for networksetup: {error}"
            ))
        })?;

    listener
        .set_nonblocking(true)
        .map_err(|error| AppError::internal(format!("configure helper listener: {error}")))?;
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => {
                let uid = peer_effective_uid(&stream).map_err(|error| {
                    AppError::internal(format!("authenticate networksetup helper: {error}"))
                })?;
                if uid == 0 {
                    stream.set_nonblocking(false).map_err(|error| {
                        AppError::internal(format!("configure helper connection: {error}"))
                    })?;
                    break stream;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => {
                return Err(AppError::internal(format!(
                    "accept authorized networksetup helper: {error}"
                )))
            }
        }
        if child
            .try_wait()
            .map_err(|error| AppError::internal(format!("wait for authorization: {error}")))?
            .is_some()
        {
            let output = child.wait_with_output().map_err(|error| {
                AppError::internal(format!("collect authorization result: {error}"))
            })?;
            return Err(authorization_failure(&output));
        }
        std::thread::sleep(Duration::from_millis(25));
    };

    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(stream)
}

fn request(stream: &mut UnixStream, commands: &[Vec<String>]) -> io::Result<HelperResponse> {
    serde_json::to_writer(
        &mut *stream,
        &HelperRequest::NetworkSetup {
            commands: commands.to_vec(),
        },
    )
    .map_err(io::Error::other)?;
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
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let request: HelperRequest = serde_json::from_str(&line).map_err(io::Error::other)?;
        let response = match request {
            HelperRequest::NetworkSetup { commands } => run_networksetup_as_root(&commands),
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
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::thread;

    use super::{
        request, validate_networksetup_command, HelperRequest, HelperResponse,
        PRIVILEGED_COMMAND_SCRIPT,
    };

    #[test]
    fn privileged_helper_accepts_only_fixed_proxy_mutations() {
        assert!(validate_networksetup_command(&[
            "-setwebproxy".into(),
            "Wi-Fi".into(),
            "127.0.0.1".into(),
            "7890".into(),
        ])
        .is_ok());
        assert!(validate_networksetup_command(&[
            "-setwebproxystate".into(),
            "Wi-Fi".into(),
            "off".into(),
        ])
        .is_ok());
        assert!(validate_networksetup_command(&["-listallnetworkservices".into()]).is_err());
        assert!(validate_networksetup_command(&["-setwebproxy".into(), "Wi-Fi".into(),]).is_err());
    }

    #[test]
    fn authorization_script_quotes_every_helper_argument() {
        assert!(PRIVILEGED_COMMAND_SCRIPT.contains("quoted form of"));
        assert!(PRIVILEGED_COMMAND_SCRIPT.contains("with administrator privileges"));
    }

    #[test]
    fn one_authorized_channel_serves_repeated_proxy_transactions() {
        let (mut parent, helper) = UnixStream::pair().expect("create helper channel");
        let server = thread::spawn(move || {
            let mut helper = helper;
            for expected_state in ["on", "off"] {
                let mut line = String::new();
                BufReader::new(&mut helper)
                    .read_line(&mut line)
                    .expect("read helper request");
                let request: HelperRequest = serde_json::from_str(&line).expect("parse request");
                let HelperRequest::NetworkSetup { commands } = request;
                assert_eq!(commands[0][2], expected_state);

                serde_json::to_writer(
                    &mut helper,
                    &HelperResponse {
                        success: true,
                        code: 0,
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    },
                )
                .expect("write helper response");
                helper.write_all(b"\n").expect("terminate response");
                helper.flush().expect("flush response");
            }
        });

        for state in ["on", "off"] {
            let commands = vec![vec![
                "-setwebproxystate".into(),
                "Wi-Fi".into(),
                state.into(),
            ]];
            assert!(
                request(&mut parent, &commands)
                    .expect("request proxy transaction")
                    .success
            );
        }
        server.join().expect("helper server");
    }
}
