use super::*;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

fn limits() -> libc::rlimit {
    let mut limits = std::mem::MaybeUninit::uninit();
    assert_eq!(
        unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, limits.as_mut_ptr()) },
        0
    );
    unsafe { limits.assume_init() }
}

#[test]
fn kernel_child_drops_foreign_fds_and_preserves_stdio_and_parent_limits() {
    const CHILD_ENV: &str = "ZNET_TEST_KERNEL_FD";
    if let Ok(value) = std::env::var(CHILD_ENV) {
        let fd: i32 = value.parse().unwrap();
        assert_eq!(unsafe { libc::fcntl(fd, libc::F_GETFD) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EBADF)
        );
        for fd in 0..=2 {
            assert_ne!(unsafe { libc::fcntl(fd, libc::F_GETFD) }, -1);
        }
        let limits = limits();
        assert!(limits.rlim_cur >= 4096.min(limits.rlim_max));
        println!("kernel descriptor isolation verified");
        return;
    }
    let before = limits();
    let file = std::fs::File::open("/dev/null").unwrap();
    // F_DUPFD deliberately omits CLOEXEC, as macOS UI frameworks can do.
    let raw = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_DUPFD, 150) };
    assert!(raw >= 150);
    let foreign = unsafe { OwnedFd::from_raw_fd(raw) };
    let output = command(std::env::current_exe().unwrap().to_str().unwrap())
        .args(["--exact", "services::kernel_command::tests::kernel_child_drops_foreign_fds_and_preserves_stdio_and_parent_limits", "--nocapture"])
        .env(CHILD_ENV, foreign.as_raw_fd().to_string())
        .output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("kernel descriptor isolation verified")
    );
    assert_ne!(
        unsafe { libc::fcntl(foreign.as_raw_fd(), libc::F_GETFD) },
        -1
    );
    let after = limits();
    assert_eq!(
        (before.rlim_cur, before.rlim_max),
        (after.rlim_cur, after.rlim_max)
    );
}

#[test]
fn failed_exec_is_still_reported_and_hard_limits_are_respected() {
    assert_eq!(
        command("/znet-test-nonexistent-kernel")
            .spawn()
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::NotFound
    );
    assert_eq!(requested_limit(256, 1024), 1024);
    assert_eq!(requested_limit(256, 65536), 4096);
    assert_eq!(requested_limit(16384, 65536), 16384);
}
