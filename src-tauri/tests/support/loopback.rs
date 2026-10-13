use std::process::Command;

pub fn isolated_loopback_probe(test: &str) -> bool {
    const CHILD: &str = "ZNET_NETWORK_PROBE_TEST_CHILD";
    if std::env::var(CHILD).as_deref() == Ok(test) {
        return true;
    }
    // Production requests intentionally follow system/environment proxies.
    // Bypass them only for this child's local fixtures. Changing process-wide
    // environment variables would race other tests using the HTTP executor.
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env(CHILD, test)
        .env("NO_PROXY", "127.0.0.1,localhost,::1")
        .env("no_proxy", "127.0.0.1,localhost,::1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated probe test failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "isolated probe test did not run: {test}"
    );
    false
}
