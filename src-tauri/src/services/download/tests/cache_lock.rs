use super::*;

fn lock_file(path: &Path) -> File {
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .unwrap()
}

#[cfg(unix)]
#[test]
fn dropping_cache_unlocks_while_a_duplicate_descriptor_is_still_open() {
    let root = tempfile::tempdir().unwrap();
    let cache = Cache::open(root.path(), "http://local/release", "v1").unwrap();
    // dup and fork retain references to the same Unix open file description.
    let duplicate = cache._lock.file.try_clone().unwrap();
    assert!(Cache::open(root.path(), "http://local/release", "v1").is_err());
    drop(cache);

    let next = Cache::open(root.path(), "http://local/release", "v1").unwrap();
    drop(duplicate);
    assert!(Cache::open(root.path(), "http://local/release", "v1").is_err());
    drop(next);
    assert!(Cache::open(root.path(), "http://local/release", "v1").is_ok());
}

#[cfg(unix)]
#[test]
fn dropping_a_fork_inherited_guard_does_not_unlock_its_parent() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("lock");
    let lock = CacheLock::try_acquire(lock_file(&path)).unwrap();
    // The child only drops this File/u32 guard (getpid and close), then _exit.
    // It must not allocate, unwind, or drop the surrounding TempDir after fork.
    let child = unsafe { libc::fork() };
    assert!(child >= 0, "{}", std::io::Error::last_os_error());
    if child == 0 {
        drop(lock);
        unsafe { libc::_exit(0) };
    }
    let mut status = 0;
    assert_eq!(unsafe { libc::waitpid(child, &mut status, 0) }, child);
    assert_eq!(status, 0);
    assert!(matches!(
        lock_file(&path).try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    drop(lock);
    assert!(CacheLock::try_acquire(lock_file(&path)).is_ok());
}

#[test]
fn failed_lock_acquisition_does_not_unlock_the_current_owner() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("lock");
    let owner = CacheLock::try_acquire(lock_file(&path)).unwrap();
    assert!(matches!(
        CacheLock::try_acquire(lock_file(&path)),
        Err(TryLockError::WouldBlock)
    ));
    assert!(matches!(
        lock_file(&path).try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    drop(owner);
    assert!(CacheLock::try_acquire(lock_file(&path)).is_ok());
}

fn expire(cache: &Cache) {
    let old = SystemTime::now() - Duration::from_secs(8 * 86400);
    lock_file(&cache.root.join("metadata.json"))
        .set_times(std::fs::FileTimes::new().set_modified(old))
        .unwrap();
}

#[test]
fn prune_preserves_active_ownership_and_releases_its_cleanup_lock() {
    let root = tempfile::tempdir().unwrap();
    let held = Cache::open(root.path(), "http://local/held", "v1").unwrap();
    fs::write(&held.part, b"active").unwrap();
    expire(&held);
    let stale = Cache::open(root.path(), "http://local/stale", "v1").unwrap();
    fs::write(&stale.part, b"expired").unwrap();
    expire(&stale);
    let stale_root = stale.root.clone();
    drop(stale);

    prune(root.path());
    assert_eq!(fs::read(&held.part).unwrap(), b"active");
    assert!(matches!(
        CacheLock::try_acquire(lock_file(&held.root.join("lock"))),
        Err(TryLockError::WouldBlock)
    ));
    assert!(!stale_root.join("payload.part").exists());
    assert!(!stale_root.join("metadata.json").exists());
    assert!(stale_root.join("lock").exists());
    assert!(Cache::open(root.path(), "http://local/stale", "v1").is_ok());
}

#[test]
fn lock_errors_preserve_io_diagnostics_instead_of_reporting_contention() {
    let error = lock_error(TryLockError::Error(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "lock permission denied",
    )));
    assert!(error.message.contains("lock permission denied"));
    assert!(!error.message.contains("正在下载或校验"));
    assert!(lock_error(TryLockError::WouldBlock)
        .message
        .contains("正在下载或校验"));
}
