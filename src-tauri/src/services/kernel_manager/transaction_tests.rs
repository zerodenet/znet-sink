use super::*;

#[test]
fn interrupted_upgrade_blocks_launch_and_overwrite_even_with_same_owner_pid() {
    let dir = tempfile::tempdir().unwrap();
    let backups = dir.path().join("backups");
    fs::write(dir.path().join("zero"), b"old").unwrap();
    let transaction = BundleTransaction::prepare(dir.path(), &backups, &["zero".into()]).unwrap();
    ensure_no_interrupted_upgrade(&backups).unwrap();
    drop(transaction); // Losing the live transaction cannot be hidden by owner_pid equality.
    let error = ensure_no_interrupted_upgrade(&backups).unwrap_err();
    assert_eq!(error.code, "kernel_upgrade_recovery_required");
    assert!(error.details.unwrap()["backupPath"].is_string());
    assert!(BundleTransaction::prepare(dir.path(), &backups, &["zero".into()]).is_err());
    assert_eq!(fs::read(dir.path().join("zero")).unwrap(), b"old");
}

#[test]
fn completed_or_rolled_back_upgrades_allow_the_next_launch() {
    for commit in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let backups = dir.path().join("backups");
        let mut transaction =
            BundleTransaction::prepare(dir.path(), &backups, &["zero".into()]).unwrap();
        if commit {
            transaction.commit().unwrap();
        } else {
            transaction.rollback().unwrap();
        }
        drop(transaction);
        ensure_no_interrupted_upgrade(&backups).unwrap();
    }
}

#[test]
fn cancelled_preparation_never_rewrites_the_running_installation() {
    let dir = tempfile::tempdir().unwrap();
    let backups = dir.path().join("backups");
    let binary = dir.path().join("zero");
    fs::write(&binary, b"old").unwrap();
    let mut transaction =
        BundleTransaction::prepare(dir.path(), &backups, &["zero".into()]).unwrap();
    fs::create_dir(transaction.backup_path().join("previous-app-config.json")).unwrap();
    assert!(transaction
        .preserve_config(&crate::models::app_config::AppConfig::default())
        .is_err());
    fs::write(&binary, b"still owned externally").unwrap();
    transaction.cancel_prepared().unwrap();
    drop(transaction);
    ensure_no_interrupted_upgrade(&backups).unwrap();
    assert_eq!(fs::read(binary).unwrap(), b"still owned externally");
}

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
