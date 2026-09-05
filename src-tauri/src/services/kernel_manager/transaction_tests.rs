use super::*;

#[test]
fn failed_upgrade_restores_binary_companions_and_manifest_and_removes_new_files() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("core");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("zero"), b"old binary").unwrap();
    fs::write(target.join("wintun.dll"), b"old companion").unwrap();
    fs::write(target.join("manifest"), b"old manifest").unwrap();
    fs::write(target.join("personal-file"), b"leave alone").unwrap();
    let names = ["zero", "wintun.dll", "manifest", "new-companion"].map(str::to_owned);
    let mut transaction =
        BundleTransaction::prepare(&target, &dir.path().join("backups"), &names).unwrap();
    for name in &names {
        fs::write(target.join(name), b"candidate").unwrap();
    }
    transaction.rollback().unwrap();
    assert_eq!(fs::read(target.join("zero")).unwrap(), b"old binary");
    assert_eq!(
        fs::read(target.join("wintun.dll")).unwrap(),
        b"old companion"
    );
    assert_eq!(fs::read(target.join("manifest")).unwrap(), b"old manifest");
    assert_eq!(
        fs::read(target.join("personal-file")).unwrap(),
        b"leave alone"
    );
    assert!(!target.join("new-companion").exists());
    transaction.rollback().unwrap();
}

#[test]
fn successful_upgrade_retains_recoverable_previous_version() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("core");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("zero"), b"old binary").unwrap();
    let mut transaction =
        BundleTransaction::prepare(&target, &dir.path().join("backups"), &["zero".into()]).unwrap();
    let backup = transaction.backup_path().to_owned();
    fs::write(target.join("zero"), b"new binary").unwrap();
    transaction.commit().unwrap();
    drop(transaction);
    assert_eq!(fs::read(backup.join("files/zero")).unwrap(), b"old binary");
    let receipt: Receipt =
        serde_json::from_slice(&fs::read(backup.join("receipt.json")).unwrap()).unwrap();
    assert_eq!(receipt.state, "committed");
}

#[test]
fn backup_failure_cannot_change_the_working_installation() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("zero"), b"old binary").unwrap();
    assert!(
        BundleTransaction::prepare(dir.path(), &dir.path().join("zero"), &["zero".into()]).is_err()
    );
    assert_eq!(fs::read(dir.path().join("zero")).unwrap(), b"old binary");
}

#[test]
fn incomplete_rollback_retains_pending_receipt_and_backup() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("zero"), b"old binary").unwrap();
    let mut transaction =
        BundleTransaction::prepare(dir.path(), &dir.path().join("backups"), &["zero".into()])
            .unwrap();
    fs::remove_file(dir.path().join("zero")).unwrap();
    fs::create_dir(dir.path().join("zero")).unwrap();
    assert!(transaction.rollback().is_err());
    assert_eq!(
        fs::read(transaction.backup_path().join("files/zero")).unwrap(),
        b"old binary"
    );
    let receipt: Receipt =
        serde_json::from_slice(&fs::read(transaction.backup_path().join("receipt.json")).unwrap())
            .unwrap();
    assert_eq!(receipt.state, "pending");
}

#[cfg(unix)]
#[test]
fn rollback_preserves_executable_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("zero");
    fs::write(&binary, b"old binary").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let mut transaction =
        BundleTransaction::prepare(dir.path(), &dir.path().join("backups"), &["zero".into()])
            .unwrap();
    // Even byte-identical reinstall must restore the previous permission mode.
    fs::write(&binary, b"old binary").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o600)).unwrap();
    transaction.rollback().unwrap();
    assert_eq!(
        fs::metadata(&binary).unwrap().permissions().mode() & 0o777,
        0o755
    );
}
