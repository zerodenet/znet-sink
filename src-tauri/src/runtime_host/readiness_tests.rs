use super::*;

#[cfg(unix)]
#[path = "readiness_real_tests.rs"]
mod real;

#[test]
fn delayed_ipc_readiness_is_retried_before_reporting_success() {
    let mut attempts = 0;
    wait(
        || Ok(true),
        |_| {
            attempts += 1;
            if attempts < 3 {
                Err(AppError::internal("IPC not ready"))
            } else {
                Ok(())
            }
        },
        Duration::from_secs(1),
        Duration::from_millis(1),
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(attempts, 3);
}

#[test]
fn alive_process_without_ipc_is_not_ready() {
    let result = wait(
        || Ok(true),
        |_| Err(AppError::internal("no IPC")),
        Duration::from_millis(10),
        Duration::from_millis(1),
        Duration::ZERO,
    );
    assert!(result.unwrap_err().message.contains("timed out"));
}

#[test]
fn process_exit_during_successful_probe_is_rejected() {
    let mut alive_checks = 0;
    let result = wait(
        || {
            alive_checks += 1;
            Ok(alive_checks == 1)
        },
        |_| Ok(()),
        Duration::from_secs(1),
        Duration::from_millis(1),
        Duration::ZERO,
    );
    assert!(result.unwrap_err().message.contains("exited"));
}

#[cfg(unix)]
#[test]
fn healthy_ipc_for_another_pid_is_rejected() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("core.sock");
    let server = UnixListener::bind(&path).unwrap();
    let worker = std::thread::spawn(move || {
        for response in [
            json!({"health": {"healthy":true}}),
            json!({"runtime": {"pid":999}}),
        ] {
            let (mut stream, _) = server.accept().unwrap();
            let mut request = String::new();
            BufReader::new(&stream).read_line(&mut request).unwrap();
            writeln!(stream, "{}", json!({"ok":true,"result":response})).unwrap();
        }
    });
    let endpoint = CoreEndpoint {
        transport: "unix-socket".into(),
        path: path.to_string_lossy().into_owned(),
    };
    assert!(probe(&endpoint, 123, Duration::from_secs(1))
        .unwrap_err()
        .message
        .contains("different process"));
    worker.join().unwrap();
}

#[test]
fn transient_health_before_startup_exit_is_not_ready() {
    let mut checks = 0;
    let result = wait(
        || {
            checks += 1;
            Ok(checks < 5)
        },
        |_| Ok(()),
        Duration::from_secs(1),
        Duration::from_millis(1),
        Duration::from_millis(100),
    );
    assert!(result.unwrap_err().message.contains("exited"));
}
