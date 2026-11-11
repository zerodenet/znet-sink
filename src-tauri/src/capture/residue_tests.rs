use super::*;

#[test]
fn residual_adapter_or_unreadable_state_never_admits_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let roots = [dir.path().to_owned()];
    assert!(audit("owned-tun", &roots, |_| Ok(true)).is_err());
    assert!(audit("owned-tun", &roots, |_| Err(AppError::internal(
        "native audit failed"
    )))
    .is_err());
    let unreadable = dir.path().join("file");
    std::fs::write(&unreadable, "not a directory").unwrap();
    assert!(audit("owned-tun", &[unreadable], |_| Ok(false)).is_err());
}

#[test]
fn outstanding_route_journal_blocks_every_retry_without_mutating_unowned_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes-znet-sink-tun-v4.json");
    let bytes = b"unconfirmed recovery, including an unreadable old schema";
    std::fs::write(&path, bytes).unwrap();
    let roots = [dir.path().to_owned()];
    for _ in 0..6 {
        assert!(audit("owned-tun", &roots, |_| Ok(false)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    std::fs::remove_file(&path).unwrap();
    audit("owned-tun", &roots, |_| Ok(false)).unwrap();
}

#[test]
fn stale_lock_metadata_is_not_misreported_as_a_live_route_journal() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("routes-v4.owner.lock"), "").unwrap();
    std::fs::write(dir.path().join("routes-v4.owner"), "previous owner").unwrap();
    audit("owned-tun", &[dir.path().to_owned()], |_| Ok(false)).unwrap();
}

#[test]
fn bounded_audit_waits_for_actual_journal_removal_without_deleting_it_itself() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes-owned-v4.json");
    std::fs::write(&path, "pending").unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let owner_path = path.clone();
    let owner = std::thread::spawn(move || {
        rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        std::fs::remove_file(owner_path).unwrap();
    });
    let mut submitted = Some(tx);
    wait_absent(std::time::Duration::from_secs(2), || {
        let result = audit("owned", &[dir.path().to_owned()], |_| Ok(false));
        if result.is_err() {
            if let Some(tx) = submitted.take() {
                tx.send(()).unwrap();
            }
        }
        result
    })
    .unwrap();
    owner.join().unwrap();
    assert!(!path.exists());
}

#[test]
fn expired_audit_retains_the_residual_and_returns_its_failure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routes-owned-v6.json");
    std::fs::write(&path, "pending").unwrap();
    let result = wait_absent(std::time::Duration::from_millis(30), || {
        audit("owned", &[dir.path().to_owned()], |_| Ok(false))
    });
    assert_eq!(result.unwrap_err().code, "tun_cleanup_unconfirmed");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "pending");
}

#[cfg(windows)]
#[test]
fn native_windows_interface_audit_is_read_only_and_reports_absent_unique_name() {
    let name = format!("ZNetAbsent-{}", std::process::id());
    assert!(!adapter_present(&name).unwrap());
}
