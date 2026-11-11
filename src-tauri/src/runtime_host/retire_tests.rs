use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Command, Stdio},
};

#[path = "retire_control_tests.rs"]
mod control;

// Run this actual test executable as the owned kernel fixture on Unix and
// Windows alike. Its private TCP channel proves it remains responsive while
// cleanup is pending; no shell, process-name kill or privileged TUN is used.
#[test]
fn child_fixture() {
    let Ok(address) = std::env::var("ZNET_RETIRE_TEST_CHILD") else {
        return;
    };
    let mut control = TcpStream::connect(address).unwrap();
    control.write_all(b"ready").unwrap();
    let worker = std::thread::spawn(move || {
        let mut byte = [0];
        while control.read_exact(&mut byte).is_ok() {
            if control.write_all(&byte).is_err() {
                break;
            }
        }
    });
    if std::env::var_os("ZNET_RETIRE_TEST_IGNORE_EOF").is_some() {
        worker.join().unwrap();
    } else {
        std::io::stdin().read_to_end(&mut Vec::new()).unwrap();
    }
    std::process::exit(0);
}

struct Fixture {
    child: Child,
    control: TcpStream,
}

impl Fixture {
    fn spawn(ignore_eof: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "runtime_host::retire::tests::child_fixture",
                "--nocapture",
            ])
            .env(
                "ZNET_RETIRE_TEST_CHILD",
                listener.local_addr().unwrap().to_string(),
            )
            .env_remove("ZNET_RETIRE_TEST_IGNORE_EOF")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        if ignore_eof {
            command.env("ZNET_RETIRE_TEST_IGNORE_EOF", "1");
        }
        let mut child = crate::runtime_host::child::spawn(&mut command).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut control = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline || child.try_wait().unwrap().is_some() {
                        let _ = child.kill();
                        let _ = child.wait();
                        panic!("child did not publish its real control channel");
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept child: {error}"),
            }
        };
        control.set_nonblocking(false).unwrap();
        control
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        control
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut ready = [0; 5];
        control.read_exact(&mut ready).unwrap();
        assert_eq!(&ready, b"ready");
        Self { child, control }
    }

    fn ping(&mut self) {
        self.control.write_all(b"p").unwrap();
        let mut reply = [0];
        self.control.read_exact(&mut reply).unwrap();
        assert_eq!(&reply, b"p");
        assert!(self.child.try_wait().unwrap().is_none());
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn retirement_keeps_the_child_alive_until_tun_cleanup_finishes_then_reaps_it() {
    let mut owned = Fixture::spawn(false);
    let pid = owned.child.id();
    let mut external = Fixture::spawn(false);
    let mut control = owned.control.try_clone().unwrap();
    let retirement = retire_with_cleanup(&mut owned.child, Duration::from_secs(2), |actual| {
        assert_eq!(actual, pid);
        // The actual child still has its parent-lifetime pipe and answers I/O
        // during cleanup. Closing the pipe first would make this fail.
        control.write_all(b"p").unwrap();
        let mut reply = [0];
        control.read_exact(&mut reply).unwrap();
        assert_eq!(&reply, b"p");
        Ok(())
    });
    retirement.tun_cleanup.unwrap();
    assert!(retirement.exit.unwrap().success());
    assert!(owned.child.try_wait().unwrap().is_some());
    external.ping();
}

#[test]
fn lost_tun_cleanup_acknowledgement_is_retained_even_after_child_exit() {
    let mut owned = Fixture::spawn(false);
    let retirement = retire_with_cleanup(&mut owned.child, Duration::from_secs(2), |_| {
        Err(AppError::internal("route cleanup acknowledgement lost"))
    });
    assert!(retirement.exit.unwrap().success());
    assert!(retirement
        .tun_cleanup
        .unwrap_err()
        .message
        .contains("acknowledgement lost"));
    assert!(owned.child.try_wait().unwrap().is_some());
}

#[test]
fn unresponsive_owned_child_is_terminated_and_reaped_without_touching_another_process() {
    let mut owned = Fixture::spawn(true);
    let mut external = Fixture::spawn(false);
    owned.ping();
    let exit = stop_owned_child(&mut owned.child, Duration::from_millis(50)).unwrap();
    assert!(!exit.success());
    assert!(owned.child.try_wait().unwrap().is_some());
    external.ping();
}

#[test]
fn confirmed_crashed_child_does_not_receive_a_second_tun_cleanup() {
    let mut owned = Fixture::spawn(false);
    owned.child.kill().unwrap();
    owned.child.wait().unwrap();
    let retirement = retire_with_cleanup(&mut owned.child, Duration::ZERO, |_| {
        panic!("a dead child has no owned IPC peer")
    });
    assert!(!retirement.exit.unwrap().success());
    assert!(retirement.tun_cleanup.is_err());
}

#[test]
fn retained_child_blocks_spawn_even_after_native_exit_until_receipt_is_consumed() {
    let mut owned = Fixture::spawn(false);
    let child = &mut owned.child;
    child.stdin.take();
    child.wait().unwrap();
    // Move an actual child into the same host slot used by production spawn.
    let mut slot = crate::runtime_host::ManagedCoreProcess::default();
    let spare = crate::runtime_host::child::spawn(
        Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .stdout(Stdio::null()),
    )
    .unwrap();
    slot.child = Some(spare);
    std::mem::swap(&mut owned.child, slot.child.as_mut().unwrap());
    assert!(slot.ensure_spawn_allowed().is_err());
    assert!(slot.child.as_mut().unwrap().try_wait().unwrap().is_some());
    slot.child.take();
    slot.ensure_spawn_allowed().unwrap();
}

#[test]
fn wait_failure_preserves_the_actual_child_and_blocks_replacement_until_retry_reaps_it() {
    let mut owned = Fixture::spawn(false);
    let retirement = retire_with(
        &mut owned.child,
        |_| Err(AppError::internal("cleanup reply lost")),
        |_| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected wait failure",
            ))
        },
    );
    assert!(retirement.exit.is_err());
    assert!(retirement.tun_cleanup.is_err());
    owned.ping();
    let mut slot = crate::runtime_host::ManagedCoreProcess::default();
    let spare = crate::runtime_host::child::spawn(
        Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .stdout(Stdio::null()),
    )
    .unwrap();
    slot.child = Some(spare);
    std::mem::swap(&mut owned.child, slot.child.as_mut().unwrap());
    assert!(slot.ensure_spawn_allowed().is_err());
    let status = stop_owned_child(slot.child.as_mut().unwrap(), Duration::from_secs(2)).unwrap();
    assert!(status.success());
    assert!(slot.child.as_mut().unwrap().try_wait().unwrap().is_some());
    slot.child.take();
    slot.ensure_spawn_allowed().unwrap();
}

#[test]
fn repeated_generations_consume_each_original_exit_before_the_next_spawn() {
    let mut external = Fixture::spawn(false);
    for generation in 0..6 {
        let mut owned = Fixture::spawn(generation % 2 == 1);
        owned.ping();
        let retirement =
            retire_with_cleanup(&mut owned.child, Duration::from_millis(100), |_| Ok(()));
        retirement.tun_cleanup.unwrap();
        let exit = retirement.exit.unwrap();
        assert_eq!(exit.success(), generation % 2 == 0);
        assert!(owned.child.try_wait().unwrap().is_some());
        external.ping();
    }
}
