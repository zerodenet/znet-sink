#[cfg(unix)]
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
