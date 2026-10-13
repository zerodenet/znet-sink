//! Authorization launches a helper; the private channel owns its lifetime.
use std::{
    io::Read,
    os::unix::net::{UnixListener, UnixStream},
    process::{Child, Command, Output, Stdio},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use super::{
    authorization_failure, bind_helper_socket, peer_effective_uid,
    require_interactive_authorization, HELPER_COMMAND, SHUTTING_DOWN,
};
use crate::errors::{AppError, AppResult};

pub(super) const PRIVILEGED_COMMAND_SCRIPT: &str = r#"
on run argv
    if (count of argv) is 0 then error "missing privileged command"
    set commandText to quoted form of (item 1 of argv)
    repeat with argumentIndex from 2 to (count of argv)
        set commandText to commandText & " " & quoted form of (item argumentIndex of argv)
    end repeat
    do shell script commandText & " </dev/null >/dev/null 2>&1 &" with administrator privileges
end run
"#;

pub(super) fn launch_authorized_helper() -> AppResult<UnixStream> {
    require_interactive_authorization(!SHUTTING_DOWN.load(Ordering::SeqCst))?;
    let (listener, guard) = bind_helper_socket().map_err(|error| {
        AppError::internal(format!("prepare macOS networksetup helper: {error}"))
    })?;
    let executable = std::env::current_exe().map_err(|error| {
        AppError::internal(format!(
            "resolve ZNet Sink executable for authorization: {error}"
        ))
    })?;
    // Quit may have started while preparing the socket and executable path.
    require_interactive_authorization(!SHUTTING_DOWN.load(Ordering::SeqCst))?;
    let mut child = Command::new("/usr/bin/osascript")
        .args(["-e", PRIVILEGED_COMMAND_SCRIPT, "--"])
        .arg(executable)
        .args([HELPER_COMMAND, "--socket"])
        .arg(&guard.socket)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            AppError::internal(format!(
                "request administrator authorization for networksetup: {error}"
            ))
        })?;
    let result = await_helper(&listener, &mut child, 0, Duration::from_secs(5));
    // The authorizer completes after launching, independently of helper life.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    result
}

fn await_helper(
    listener: &UnixListener,
    child: &mut Child,
    expected_uid: libc::uid_t,
    connect_timeout: Duration,
) -> AppResult<UnixStream> {
    listener
        .set_nonblocking(true)
        .map_err(|error| AppError::internal(format!("configure helper listener: {error}")))?;
    let mut deadline = None;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| AppError::internal(format!("wait for authorization: {error}")))?
        {
            if !status.success() {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = pipe.read_to_end(&mut stdout);
                }
                if let Some(mut pipe) = child.stderr.take() {
                    let _ = pipe.read_to_end(&mut stderr);
                }
                return Err(authorization_failure(&Output {
                    status,
                    stdout,
                    stderr,
                }));
            }
            // Do not time out the user while they are entering credentials.
            // Only the connection after successful authorization is bounded.
            deadline.get_or_insert_with(|| Instant::now() + connect_timeout);
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let uid = peer_effective_uid(&stream).map_err(|error| {
                    AppError::internal(format!("authenticate networksetup helper: {error}"))
                })?;
                if uid == expected_uid {
                    stream.set_nonblocking(false).map_err(|error| {
                        AppError::internal(format!("configure helper connection: {error}"))
                    })?;
                    return Ok(stream);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => {
                return Err(AppError::internal(format!(
                    "accept authorized networksetup helper: {error}"
                )))
            }
        }
        if deadline.is_some_and(|at| Instant::now() >= at) {
            return Err(AppError::internal(
                "authorized macOS network helper did not connect",
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(test)]
#[path = "launch_tests.rs"]
mod tests;
