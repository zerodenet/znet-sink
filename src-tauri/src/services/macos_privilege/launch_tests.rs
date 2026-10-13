use std::{
    io::Write,
    os::{
        fd::AsRawFd,
        unix::net::{UnixListener, UnixStream},
    },
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use super::{await_helper, PRIVILEGED_COMMAND_SCRIPT};

fn authorizer(script: &str) -> Child {
    let mut child = Command::new("/bin/sh")
        .args(["-c", script])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.wait().unwrap();
    child
}

#[test]
fn completed_authorization_still_waits_for_the_helper_and_uses_a_blocking_channel() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("helper.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let mut child = authorizer("exit 0");
    let helper = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        UnixStream::connect(path).unwrap()
    });
    let stream = await_helper(
        &listener,
        &mut child,
        unsafe { libc::geteuid() },
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(
        unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_GETFL) } & libc::O_NONBLOCK,
        0
    );
    drop(helper.join().unwrap());
}

#[test]
fn missing_helper_times_out_after_authorization_completes() {
    let dir = tempfile::tempdir().unwrap();
    let listener = UnixListener::bind(dir.path().join("helper.sock")).unwrap();
    let mut child = authorizer("exit 0");
    let error = await_helper(
        &listener,
        &mut child,
        unsafe { libc::geteuid() },
        Duration::ZERO,
    )
    .unwrap_err();
    assert!(error.message.contains("did not connect"));
}

#[test]
fn cancelled_authorization_is_not_accepted_even_when_a_peer_is_connected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("helper.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let _peer = UnixStream::connect(path).unwrap();
    let mut child = authorizer("printf 'User canceled. (-128)' >&2; exit 1");
    let error = await_helper(
        &listener,
        &mut child,
        unsafe { libc::geteuid() },
        Duration::ZERO,
    )
    .unwrap_err();
    assert_eq!(error.code, "authorization_cancelled");
}

#[test]
fn helper_peer_must_match_the_required_uid() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("helper.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let _peer = UnixStream::connect(path).unwrap();
    let mut child = authorizer("exit 0");
    let error = await_helper(
        &listener,
        &mut child,
        unsafe { libc::geteuid() }.wrapping_add(1),
        Duration::ZERO,
    )
    .unwrap_err();
    assert!(error.message.contains("did not connect"));
}

#[test]
fn authorizer_exits_while_the_detached_helper_is_still_running() {
    let dir = tempfile::Builder::new()
        .prefix("znet auth '")
        .tempdir()
        .unwrap();
    let ready = dir.path().join("ready");
    let control = dir.path().join("control");
    let finished = dir.path().join("finished");
    let fifo = std::ffi::CString::new(control.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    // Use the real AppleScript quoting/launch path without requesting admin
    // authority or changing host networking. A FIFO holds the helper fixture.
    let script = PRIVILEGED_COMMAND_SCRIPT.replace(" with administrator privileges", "");
    let fixture = "printf ready > \"$1\"; IFS= read -r command < \"$2\"; printf closed > \"$3\"";
    let mut child = Command::new("/usr/bin/osascript")
        .args(["-e", &script, "--", "/bin/sh", "-c", fixture, "--"])
        .arg(&ready)
        .arg(&control)
        .arg(&finished)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready.exists() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!(
                "helper did not start: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut exited = None;
    while Instant::now() < deadline {
        exited = child.try_wait().unwrap();
        if exited.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let still_running = !finished.exists();
    // Release the fixture before assertions so a failed check cannot strand it.
    std::fs::OpenOptions::new()
        .write(true)
        .open(&control)
        .unwrap()
        .write_all(b"close\n")
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !finished.exists() {
        assert!(
            Instant::now() < deadline,
            "helper did not stop on control input"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    child.wait().unwrap();
    assert!(still_running);
    assert!(
        exited.is_some_and(|status| status.success()),
        "authorizer must not wait for helper lifetime"
    );
}
