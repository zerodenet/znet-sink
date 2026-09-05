use super::*;

#[test]
fn interrupted_legacy_save_recovers_previous_config_and_preserves_corrupt_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app-config.json");
    let mut config = AppConfig::default();
    config.local_proxy.port = 17890;
    save(&path, &config).unwrap();
    config.local_proxy.port = 17891;
    save(&path, &config).unwrap();
    fs::write(&path, b"{truncated").unwrap();
    assert_eq!(load_or_default(&path).unwrap().local_proxy.port, 17890);
    assert_eq!(read(&path).unwrap().local_proxy.port, 17890);
    let preserved = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("app-config-corrupt-")
        })
        .unwrap();
    assert_eq!(fs::read(preserved.path()).unwrap(), b"{truncated");
}

#[test]
fn backup_failure_leaves_primary_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app-config.json");
    let mut config = AppConfig::default();
    save(&path, &config).unwrap();
    let before = fs::read(&path).unwrap();
    fs::remove_file(backup_path(&path)).unwrap();
    fs::create_dir(backup_path(&path)).unwrap();
    config.local_proxy.port = 17891;
    assert!(save(&path, &config).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn missing_primary_recovers_backup_instead_of_resetting_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app-config.json");
    let mut config = AppConfig::default();
    config.local_proxy.port = 17890;
    save(&path, &config).unwrap();
    fs::remove_file(&path).unwrap();
    assert_eq!(load_or_default(&path).unwrap().local_proxy.port, 17890);
}

#[test]
fn corrupt_config_without_backup_is_not_silently_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app-config.json");
    fs::write(&path, b"{truncated").unwrap();
    assert!(load_or_default(&path).is_err());
    assert!(save(&path, &AppConfig::default()).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"{truncated");
}
