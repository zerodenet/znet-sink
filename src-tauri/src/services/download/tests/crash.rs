use super::*;
use std::process::{Child, Command, Stdio};
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn crash_writer() {
    let Some(root) = std::env::var_os("ZNET_DOWNLOAD_TEST_CHILD_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let url = std::env::var("ZNET_DOWNLOAD_TEST_CHILD_URL").unwrap();
    let mut cache = Cache::open(&root, &url, "version:platform:signature").unwrap();
    std::fs::write(&cache.part, b"abcd").unwrap();
    cache.meta.etag = Some("\"v1\"".into());
    cache.meta.total = Some(8);
    cache.save().unwrap();
    std::fs::write(root.join("ready"), b"ready").unwrap();
    // The parent kills this process while it still owns the OS file lock.
    thread::sleep(Duration::from_secs(30));
    panic!("parent did not terminate the download helper");
}
#[test]
fn killed_process_releases_cache_lock_and_next_process_resumes_existing_prefix() {
    let root = tempfile::tempdir().unwrap();
    let (url, requests, task) = server(vec![remaining()]);
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "services::download::tests::crash::crash_writer",
            "--nocapture",
        ])
        .env("ZNET_DOWNLOAD_TEST_CHILD_ROOT", root.path())
        .env("ZNET_DOWNLOAD_TEST_CHILD_URL", &url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut child = ChildGuard(child);
    let started = std::time::Instant::now();
    while !root.path().join("ready").exists() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "download helper did not become ready"
        );
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "download helper exited early"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(Cache::open(root.path(), &url, "version:platform:signature").is_err());
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let artifact = fetch_test(root.path(), &url).unwrap();
    assert_eq!(std::fs::read(&artifact.path).unwrap(), b"abcdefgh");
    task.join().unwrap();
    assert!(requests.lock().unwrap()[0].contains("range: bytes=4-"));
}
