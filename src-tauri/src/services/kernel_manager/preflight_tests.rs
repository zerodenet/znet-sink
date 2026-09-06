use super::*;

#[cfg(unix)]
#[test]
fn hung_candidate_is_terminated_at_deadline() {
    let started = Instant::now();
    let result = run(Path::new("/bin/sleep"), &["10"], Duration::from_millis(50));
    assert!(result.unwrap_err().message.contains("超时"));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[cfg(unix)]
#[test]
fn unsuccessful_candidate_is_rejected_even_with_version_output() {
    let result = run(
        Path::new("/bin/sh"),
        &["-c", "echo 'zero 1.2.3'; exit 1"],
        Duration::from_secs(1),
    );
    assert!(result.unwrap_err().message.contains("1.2.3"));
}

#[cfg(unix)]
#[test]
fn noisy_candidate_cannot_block_on_output_pipe() {
    let output = run(
        Path::new("/bin/sh"),
        &["-c", "head -c 100000 /dev/zero"],
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(output.len(), 64 * 1024);
}

// Use a real child executable so isolation and cleanup are exercised on all
// supported platforms, including Windows (where open files block deletion).
#[test]
fn legacy_dns_probe() {
    let Some(state) = std::env::var_os("ZERO_DNS_STATE_DIR") else {
        return;
    };
    assert_eq!(std::env::var("NO_COLOR").unwrap(), "1");
    assert_eq!(std::env::var("TERM").unwrap(), "dumb");
    let state = std::path::PathBuf::from(state);
    let file = std::fs::File::create(state.join("fake-ip.lock")).unwrap();
    file.try_lock().unwrap();
    std::fs::write(state.join("fake-ip.jsonl"), b"probe state").unwrap();
    println!("STATE={}", state.display());
    println!("\x1b[31m预检查输出 🇸🇬\x1b[0m");
    // Overlap concurrent probes while they own their leases.
    std::thread::sleep(Duration::from_millis(100));
}

#[test]
fn concurrent_legacy_probes_get_private_state_and_clean_up_after_exit() {
    let probe = || {
        run(
            &std::env::current_exe().unwrap(),
            &[
                "--exact",
                "services::kernel_manager::preflight::tests::legacy_dns_probe",
                "--nocapture",
            ],
            Duration::from_secs(10),
        )
        .unwrap()
    };
    let first = std::thread::spawn(probe);
    let second = probe();
    let first = first.join().unwrap();
    let state_path = |output: &str| {
        assert!(!output.contains('\x1b'));
        assert!(output.contains("预检查输出 🇸🇬"));
        let path = output
            .lines()
            .find_map(|line| line.strip_prefix("STATE="))
            .unwrap();
        let path = std::path::PathBuf::from(path);
        assert!(
            !path.exists(),
            "probe state must be removed after child exits"
        );
        path
    };
    assert_ne!(state_path(&first), state_path(&second));
}

#[cfg(unix)]
#[test]
fn failed_candidate_diagnostics_have_no_terminal_color_codes() {
    let error = run(
        Path::new("/bin/sh"),
        &[
            "-c",
            r"printf '\033[31m错误：Fake-IP 状态冲突\033[0m'; exit 1",
        ],
        Duration::from_secs(2),
    )
    .unwrap_err();
    assert!(error.message.contains("错误：Fake-IP 状态冲突"));
    assert!(!error.message.contains('\x1b'));
}

#[cfg(unix)]
#[test]
fn legacy_validation_preserves_original_config_and_relative_resources() {
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("legacy-zero");
    let config = directory.path().join("config.json");
    let resource = directory.path().join("rules.json");
    std::fs::write(&config, "original config").unwrap();
    std::fs::write(&resource, "original rules").unwrap();
    std::fs::write(
        &binary,
        r#"#!/bin/sh
case "$1" in
--version) echo 'zero 1.2.3';;
help) echo '--parent-lifetime-stdin';;
validate)
  test "$(cat "$2")" = 'original config' || exit 1
  test "$(cat "$(dirname "$2")/rules.json")" = 'original rules' || exit 2
  test -d "$ZERO_DNS_STATE_DIR" || exit 3
  printf 'temporary state' > "$ZERO_DNS_STATE_DIR/fake-ip.jsonl"
  ;;
*) exit 4;;
esac
"#,
    )
    .unwrap();
    validate(&binary, "1.2.3", Some(&config)).unwrap();
    assert_eq!(std::fs::read_to_string(config).unwrap(), "original config");
    assert_eq!(std::fs::read_to_string(resource).unwrap(), "original rules");
}
