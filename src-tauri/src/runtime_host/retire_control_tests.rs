//! Exercise the production retirement path over actual Unix/Windows control I/O.
use super::*;
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader},
    sync::mpsc,
};

#[path = "../capture/shutdown_test_transport.rs"]
mod transport;

#[test]
fn owned_tun_confirmation_precedes_parent_eof_and_original_child_exit() {
    for route_failure in [false, true] {
        let (listener, endpoint, _dir) = transport::listen();
        let mut owned = Fixture::spawn(false);
        let pid = owned.child.id();
        let mut control = owned.control.try_clone().unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let stream = transport::accept(listener);
            #[cfg(unix)]
            stream
                .set_read_timeout(Some(Duration::from_secs(15)))
                .unwrap();
            let mut writer = stream.try_clone().unwrap();
            let mut stopped = false;
            let mut commands = 0;
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                let frame: Value = serde_json::from_str(&line).unwrap();
                let result = match frame["type"].as_str().unwrap() {
                    "subscribe" => json!("subscribed"),
                    "query" if frame["request"].get("runtime").is_some() => {
                        json!({"runtime":{"pid":pid}})
                    }
                    "query" => json!({"tun_status":{
                        "running":!stopped,
                        "last_error": if stopped && route_failure { Some("route cleanup failed") } else { None }
                    }}),
                    "command" => {
                        assert_eq!(frame["method"], "tun.stop");
                        commands += 1;
                        started_tx.send(()).unwrap();
                        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                        stopped = true;
                        json!({"accepted":true})
                    }
                    other => panic!("unexpected {other}"),
                };
                if writeln!(
                    writer,
                    "{}",
                    json!({
                        "api_id":"zero.api.v1", "id":frame["id"], "ok":true, "result":result
                    })
                )
                .is_err()
                {
                    break;
                }
            }
            commands
        });
        let path = endpoint.path.clone();
        let worker = std::thread::spawn(move || {
            let retirement = retire_owned_child(&mut owned.child, Some(&path));
            (owned, retirement)
        });
        // Suspend the actual awaited tun.stop response, then prove the real
        // child is still alive. No timer or PID marker substitutes for I/O.
        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        if !route_failure {
            // Keep the actual submitted response pending beyond the removed
            // four-second outer deadline, then verify liveness through I/O.
            std::thread::sleep(Duration::from_millis(4200));
        }
        assert!(!worker.is_finished());
        control.write_all(b"p").unwrap();
        let mut reply = [0];
        control.read_exact(&mut reply).unwrap();
        assert_eq!(&reply, b"p");
        release_tx.send(()).unwrap();
        let (mut owned, retirement) = worker.join().unwrap();
        assert!(retirement.exit.unwrap().success());
        assert_eq!(retirement.tun_cleanup.is_err(), route_failure);
        assert!(owned.child.try_wait().unwrap().is_some());
        crate::kernel::connection::reset_endpoint(&endpoint);
        assert_eq!(server.join().unwrap(), 1);
    }
}
