use super::*;
use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct RunningImage(Child);

impl Drop for RunningImage {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn running_windows_image_is_preserved_and_unchanged_rollback_completes() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("zero.exe");
    let shell = std::env::var_os("COMSPEC").expect("Windows command processor");
    fs::copy(shell, &target).unwrap();
    let backups = dir.path().join("backups");
    let mut transaction =
        BundleTransaction::prepare(dir.path(), &backups, &["zero.exe".into()]).unwrap();
    let mut running = RunningImage(
        Command::new(&target)
            .args(["/D", "/Q", "/K"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    assert!(running.0.try_wait().unwrap().is_none());
    let candidate = dir.path().join("candidate");
    fs::write(&candidate, b"new generation").unwrap();
    let error = replace_file(&candidate, &target).unwrap_err();
    assert_eq!(error.code, "kernel_upgrade_storage_failed");
    assert_eq!(
        error.details.unwrap()["targetPath"],
        serde_json::json!(target)
    );
    assert!(running.0.try_wait().unwrap().is_none());
    transaction.rollback().unwrap();
    assert!(super::super::files_are_identical(
        &target,
        &transaction.backup_path().join("files/zero.exe")
    )
    .unwrap());
    drop(transaction);
    ensure_no_interrupted_upgrade(&backups).unwrap();

    // After the owner explicitly stops and reaps the process, publication
    // succeeds. The installer never terminates a foreign process by name.
    drop(running);
    replace_file(&candidate, &target).unwrap();
    assert_eq!(fs::read(target).unwrap(), b"new generation");
}

#[test]
fn replacement_waits_for_transient_windows_file_lock() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("zero.exe");
    let candidate = dir.path().join("candidate");
    fs::write(&target, b"old").unwrap();
    fs::write(&candidate, b"new").unwrap();
    // Permit reading/writing, but deny deletion/rename until this handle closes.
    let held = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&target)
        .unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(700));
        drop(held);
    });
    let result = replace_file(&candidate, &target);
    release.join().unwrap();
    result.unwrap();
    assert_eq!(fs::read(target).unwrap(), b"new");
}

#[test]
fn changed_locked_file_keeps_rollback_pending_until_restore_is_possible() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("zero.exe");
    let backups = dir.path().join("backups");
    fs::write(&target, b"old").unwrap();
    let mut transaction =
        BundleTransaction::prepare(dir.path(), &backups, &["zero.exe".into()]).unwrap();
    fs::write(&target, b"new").unwrap();
    let held = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&target)
        .unwrap();
    assert!(transaction.rollback().is_err());
    let receipt: Receipt =
        serde_json::from_slice(&fs::read(transaction.backup_path().join("receipt.json")).unwrap())
            .unwrap();
    assert_eq!(receipt.state, "pending");
    assert_eq!(fs::read(target.clone()).unwrap(), b"new");
    assert_eq!(
        fs::read(transaction.backup_path().join("files/zero.exe")).unwrap(),
        b"old"
    );
    drop(held);
    transaction.rollback().unwrap();
    assert_eq!(fs::read(target).unwrap(), b"old");
}
