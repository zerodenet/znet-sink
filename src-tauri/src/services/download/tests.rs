use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

fn server(responses: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/release", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let output = requests.clone();
    let task = thread::spawn(move || {
        for response in responses {
            let start = std::time::Instant::now();
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            start.elapsed() < Duration::from_secs(15),
                            "expected download request"
                        );
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            output
                .lock()
                .unwrap()
                .push(String::from_utf8(request).unwrap().to_lowercase());
            socket.write_all(response.as_bytes()).unwrap();
        }
    });
    (url, requests, task)
}
fn response(status: &str, headers: &str, body: &str) -> String {
    format!("HTTP/1.1 {status}\r\nConnection: close\r\n{headers}\r\n{body}")
}
fn partial() -> String {
    response("200 OK", "ETag: \"v1\"\r\nContent-Length: 8\r\n", "abcd")
}
fn remaining() -> String {
    response(
        "206 Partial Content",
        "ETag: \"v1\"\r\nContent-Range: bytes 4-7/8\r\nContent-Length: 4\r\n",
        "efgh",
    )
}
fn fetch_test(root: &Path, url: &str) -> AppResult<Download> {
    fetch_in(
        root,
        &Client::builder().no_proxy().build().unwrap(),
        url,
        "version:platform:signature",
        |_| {},
        Duration::ZERO,
    )
}
#[test]
fn interrupted_body_resumes_with_strong_validator_and_absolute_progress() {
    let root = tempfile::tempdir().unwrap();
    let (url, requests, task) = server(vec![partial(), remaining()]);
    let mut events = Vec::new();
    let artifact = fetch_in(
        root.path(),
        &Client::builder().no_proxy().build().unwrap(),
        &url,
        "release",
        |p| events.push(p),
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(std::fs::read(&artifact.path).unwrap(), b"abcdefgh");
    task.join().unwrap();
    let requests = requests.lock().unwrap();
    assert!(requests[1].contains("range: bytes=4-"));
    assert!(requests[1].contains("if-range: \"v1\""));
    assert!(events
        .iter()
        .any(|e| e.state == "retrying" && e.bytes_downloaded == 4));
    assert_eq!(events.last().unwrap().bytes_downloaded, 8);
}
#[test]
fn failed_operation_and_new_process_cache_open_preserve_download_then_reuse_complete_file() {
    let root = tempfile::tempdir().unwrap();
    let (url, requests, task) = server(vec![
        partial(),
        response("404 Not Found", "Content-Length: 0\r\n", ""),
        remaining(),
    ]);
    assert!(fetch_test(root.path(), &url).is_err());
    let artifact = fetch_test(root.path(), &url).unwrap();
    assert_eq!(std::fs::read(&artifact.path).unwrap(), b"abcdefgh");
    drop(artifact);
    let cached = fetch_test(root.path(), &url).unwrap();
    assert_eq!(std::fs::read(&cached.path).unwrap(), b"abcdefgh");
    task.join().unwrap();
    assert_eq!(requests.lock().unwrap().len(), 3);
}
#[test]
fn ignored_range_replaces_partial_file_instead_of_appending() {
    let root = tempfile::tempdir().unwrap();
    let (url, _, task) = server(vec![
        partial(),
        response("200 OK", "ETag: \"v2\"\r\nContent-Length: 3\r\n", "new"),
    ]);
    let artifact = fetch_test(root.path(), &url).unwrap();
    assert_eq!(std::fs::read(&artifact.path).unwrap(), b"new");
    task.join().unwrap();
}
#[test]
fn changed_validator_on_partial_response_discards_old_prefix() {
    let root = tempfile::tempdir().unwrap();
    let (url, requests, task) = server(vec![
        partial(),
        remaining().replace("v1", "v2"),
        response("200 OK", "ETag: \"v2\"\r\nContent-Length: 3\r\n", "new"),
    ]);
    let artifact = fetch_test(root.path(), &url).unwrap();
    assert_eq!(std::fs::read(&artifact.path).unwrap(), b"new");
    task.join().unwrap();
    assert!(!requests.lock().unwrap()[2].contains("\r\nrange:"));
}
#[test]
fn missing_or_weak_validator_falls_back_to_full_download() {
    for old in [
        partial().replace("ETag: \"v1\"\r\n", ""),
        partial().replace("ETag: \"v1\"", "ETag: W/\"v1\""),
    ] {
        let root = tempfile::tempdir().unwrap();
        let (url, requests, task) = server(vec![
            old,
            response("200 OK", "Content-Length: 8\r\n", "abcdefgh"),
        ]);
        let artifact = fetch_test(root.path(), &url).unwrap();
        assert_eq!(std::fs::read(&artifact.path).unwrap(), b"abcdefgh");
        task.join().unwrap();
        assert!(!requests.lock().unwrap()[1].contains("\r\nrange:"));
    }
}
#[test]
fn transient_errors_have_finite_retry_budget() {
    let root = tempfile::tempdir().unwrap();
    let (url, requests, task) = server(vec![
        response(
            "503 Unavailable",
            "Content-Length: 0\r\nRetry-After: 0\r\n",
            ""
        );
        4
    ]);
    assert!(fetch_test(root.path(), &url)
        .err()
        .unwrap()
        .message
        .contains("已保留进度"));
    task.join().unwrap();
    assert_eq!(requests.lock().unwrap().len(), 4);
}
#[test]
fn invalid_range_is_rejected_and_never_marked_complete() {
    let root = tempfile::tempdir().unwrap();
    let (url, _, task) = server(vec![response(
        "206 Partial Content",
        "Content-Range: bytes 8-4/8\r\nContent-Length: 4\r\n",
        "oops",
    )]);
    assert!(fetch_test(root.path(), &url).is_err());
    task.join().unwrap();
}
#[test]
fn artifact_identity_and_lock_prevent_version_mix_and_concurrent_writes() {
    let root = tempfile::tempdir().unwrap();
    let a = Cache::open(root.path(), "http://local/release", "v1").unwrap();
    assert!(Cache::open(root.path(), "http://local/release", "v1").is_err());
    let b = Cache::open(root.path(), "http://local/release", "v2").unwrap();
    assert_ne!(a.part, b.part);
    drop(a);
    assert!(Cache::open(root.path(), "http://local/release", "v1").is_ok());
}
#[test]
fn complete_on_disk_without_marker_handles_416_then_still_requires_caller_validation() {
    let root = tempfile::tempdir().unwrap();
    let (url, _, task) = server(vec![response(
        "416 Range Not Satisfiable",
        "ETag: \"v1\"\r\nContent-Range: bytes */8\r\nContent-Length: 0\r\n",
        "",
    )]);
    let mut cache = Cache::open(root.path(), &url, "version:platform:signature").unwrap();
    std::fs::write(&cache.part, b"abcdefgh").unwrap();
    cache.meta.etag = Some("\"v1\"".into());
    cache.meta.total = Some(8);
    cache.save().unwrap();
    drop(cache);
    let artifact = fetch_test(root.path(), &url).unwrap();
    assert_eq!(std::fs::read(&artifact.path).unwrap(), b"abcdefgh");
    task.join().unwrap();
}

#[test]
fn resumes_through_a_fresh_redirect_instead_of_persisting_temporary_location() {
    let root = tempfile::tempdir().unwrap();
    let redirect = response(
        "302 Found",
        "Location: /temporary-asset\r\nContent-Length: 0\r\n",
        "",
    );
    let (url, requests, task) = server(vec![redirect.clone(), partial(), redirect, remaining()]);
    let artifact = fetch_test(root.path(), &url).unwrap();
    assert_eq!(std::fs::read(&artifact.path).unwrap(), b"abcdefgh");
    task.join().unwrap();
    let requests = requests.lock().unwrap();
    assert!(requests[0].starts_with("get /release "));
    assert!(requests[2].starts_with("get /release "));
    assert!(requests[3].contains("range: bytes=4-"));
}

#[test]
fn rejects_oversized_download_before_writing_payload() {
    let root = tempfile::tempdir().unwrap();
    let (url, _, task) = server(vec![response(
        "200 OK",
        &format!("Content-Length: {}\r\n", MAX_BYTES + 1),
        "",
    )]);
    assert!(fetch_test(root.path(), &url)
        .err()
        .unwrap()
        .message
        .contains("512 MB"));
    task.join().unwrap();
}

#[test]
fn expired_cache_cleanup_preserves_locked_artifacts_and_unrelated_files() {
    let root = tempfile::tempdir().unwrap();
    let held = Cache::open(root.path(), "http://local/held", "v1").unwrap();
    std::fs::write(&held.part, b"active").unwrap();
    let old = std::time::SystemTime::now() - Duration::from_secs(8 * 86400);
    std::fs::OpenOptions::new()
        .write(true)
        .open(held.part.parent().unwrap().join("metadata.json"))
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(old))
        .unwrap();
    let stale = Cache::open(root.path(), "http://local/stale", "v1").unwrap();
    std::fs::write(&stale.part, b"expired").unwrap();
    let path = stale.part.clone();
    std::fs::OpenOptions::new()
        .write(true)
        .open(path.parent().unwrap().join("metadata.json"))
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(old))
        .unwrap();
    drop(stale);
    std::fs::write(root.path().join("user-file"), "keep").unwrap();
    let _next = Cache::open(root.path(), "http://local/next", "v1").unwrap();
    assert!(held.part.exists());
    assert!(!path.exists());
    assert!(root.path().join("user-file").exists());
}

#[path = "tests/crash.rs"]
mod crash;
