use super::*;

#[cfg(unix)]
fn read_fixture_line(
    reader: &mut impl std::io::BufRead,
    line: &mut String,
) -> std::io::Result<usize> {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        match reader.read_line(line) {
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            result => return result,
        }
    }
}

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
    let error = result.unwrap_err();
    assert!(error.message.contains("timed out: no IPC"));
    assert_eq!(
        error.details.unwrap()["lastProbeError"]["message"],
        "no IPC"
    );
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
    use std::io::{BufReader, Write};
    use std::os::unix::net::UnixListener;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("core.sock");
    let server = UnixListener::bind(&path).unwrap();
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = server.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request = String::new();
        read_fixture_line(&mut reader, &mut request).unwrap();
        let frame: Value = serde_json::from_str(&request).unwrap();
        assert_eq!(frame["type"], "subscribe");
        writeln!(
            stream,
            "{}",
            json!({
                "api_id":"zero.api.v1", "id":frame["id"],
                "ok":true, "result":"subscribed"
            })
        )
        .unwrap();
        for response in [
            json!({"health": {"healthy":true}}),
            json!({"runtime": {"pid":999}}),
        ] {
            request.clear();
            read_fixture_line(&mut reader, &mut request).unwrap();
            let frame: Value = serde_json::from_str(&request).unwrap();
            assert_eq!(frame["type"], "query");
            writeln!(
                stream,
                "{}",
                json!({"id":frame["id"],"ok":true,"result":response})
            )
            .unwrap();
        }
        // Rejecting the peer must also close the dedicated subscription;
        // otherwise its reader thread retains a live socket after startup.
        request.clear();
        assert_eq!(read_fixture_line(&mut reader, &mut request).unwrap(), 0);
    });
    let endpoint = CoreEndpoint {
        transport: "unix-socket".into(),
        path: path.to_string_lossy().into_owned(),
    };
    let error = probe(&endpoint, 123, Duration::from_secs(10)).unwrap_err();
    assert!(error.message.contains("different process"), "{error:?}");
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
