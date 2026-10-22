use super::*;
use crate::{
    models::app_config::AppConfig,
    services::{app_config_store, domain_store},
};
#[test]
fn actual_app_and_rules_a_b_a_keep_values_but_advance_real_history_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app-config.json");
    let mut a = AppConfig::default();
    crate::services::bypass::normalize(&mut a).unwrap();
    app_config_store::save(&path, &a).unwrap();
    let first = app_config_store::current_history(&path).unwrap();
    let bytes = fs::read(&path).unwrap();
    let mut b = a.clone();
    b.local_proxy.port = 17891;
    app_config_store::save(&path, &b).unwrap();
    app_config_store::save(&path, &a).unwrap();
    let last = app_config_store::current_history(&path).unwrap();
    assert_eq!(bytes, fs::read(&path).unwrap());
    assert_eq!(first.store_id, last.store_id);
    assert!(last.epoch > first.epoch);
    assert_eq!(app_config_store::load_or_default(&path).unwrap(), a);
    domain_store::save_rule_sets_to_dir(dir.path(), &[]).unwrap();
    let rules_before = domain_store::current_rule_set_history(dir.path()).unwrap();
    let r=serde_json::from_value(serde_json::json!({"id":"r","name":"B","enabled":false,"semanticIr":{"version":1,"rules":[]},"updatedAtUnixMs":1})).unwrap();
    domain_store::save_rule_sets_to_dir(dir.path(), &[r]).unwrap();
    domain_store::save_rule_sets_to_dir(dir.path(), &[]).unwrap();
    let rules_after = domain_store::current_rule_set_history(dir.path()).unwrap();
    assert_eq!(rules_before.store_id, rules_after.store_id);
    assert!(rules_after.epoch > rules_before.epoch);
    assert_eq!(fs::read(dir.path().join("rule-sets.json")).unwrap(), b"[]");
}
#[test]
fn actual_backup_recovery_counts_history_instead_of_resurrecting_original_token() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app-config.json");
    let mut a = AppConfig::default();
    a.local_proxy.port = 17890;
    app_config_store::save(&path, &a).unwrap();
    let token = app_config_store::current_history(&path).unwrap();
    let mut b = a.clone();
    b.local_proxy.port = 17891;
    app_config_store::save(&path, &b).unwrap();
    fs::write(&path, b"{broken").unwrap();
    let recovered = app_config_store::load_or_default(&path).unwrap();
    assert_eq!(recovered.local_proxy.port, 17890);
    assert!(app_config_store::current_history(&path).unwrap().epoch > token.epoch);
    assert!(fs::read_dir(dir.path()).unwrap().flatten().any(|e| e
        .file_name()
        .to_string_lossy()
        .starts_with("app-config-corrupt-")));
}
#[test]
fn pending_original_owner_can_only_confirm_actual_expected_bytes_without_rewriting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rule-sets.json");
    fs::write(&path, b"[]").unwrap();
    let mut guard = acquire(&path, Domain::RuleSets).unwrap();
    let before = guard.token().unwrap();
    let result: AppResult<((), WriteReceipt)> =
        guard.mutate(b"[1]", None, Some("original-op"), || {
            fs::write(&path, b"[1]").unwrap();
            Err(unavailable())
        });
    assert!(result.is_err());
    assert!(guard.token().is_err());
    drop(guard);
    let mut reopened = acquire(&path, Domain::RuleSets).unwrap();
    let metadata_before = fs::read(suffixed(&path, ".history.json")).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(reopened.owned_token("other-op", &before).is_err());
    assert!(reopened.confirm_owned("other-op", &before).is_err());
    let after = reopened.owned_token("original-op", &before).unwrap();
    assert_eq!(
        fs::read(suffixed(&path, ".history.json")).unwrap(),
        metadata_before
    );
    assert_eq!(
        reopened.confirm_owned("original-op", &before).unwrap(),
        after
    );
    assert_eq!(
        reopened.confirm_owned("original-op", &before).unwrap(),
        after
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert!(after.epoch > before.epoch);
}
#[test]
fn failure_before_file_change_advances_history_but_partial_or_unknown_write_does_not_fake_success()
{
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.json");
    fs::write(&path, b"A").unwrap();
    let mut g = acquire(&path, Domain::RuleSets).unwrap();
    let before = g.token().unwrap();
    let r: AppResult<((), WriteReceipt)> = g.mutate(b"B", None, None, || Err(unavailable()));
    assert!(r.is_err());
    let after = g.token().unwrap();
    assert!(after.epoch > before.epoch);
    assert_eq!(fs::read(&path).unwrap(), b"A");
    let r: AppResult<((), WriteReceipt)> = g.mutate(b"B", None, Some("op"), || {
        fs::write(&path, b"partial").unwrap();
        Err(unavailable())
    });
    assert!(r.is_err());
    assert!(g.token().is_err());
    assert!(g.confirm_owned("op", &after).is_err());
    drop(g);
    let mut reopened = acquire(&path, Domain::RuleSets).unwrap();
    assert!(reopened.is_pending());
    assert!(reopened.mutate(b"B", None, None, || Ok(())).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"partial");
}
#[test]
fn oversized_or_corrupt_metadata_never_resets_history_or_echoes_private_material() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.json");
    fs::write(&path, b"[]").unwrap();
    drop(acquire(&path, Domain::RuleSets).unwrap());
    let meta = suffixed(&path, ".history.json");
    for bytes in [vec![b'X'; 65537], b"secret-canary-invalid-history".to_vec()] {
        fs::write(&meta, &bytes).unwrap();
        let error = acquire(&path, Domain::RuleSets).err().unwrap();
        assert!(!serde_json::to_string(&error).unwrap().contains("canary"));
        assert_eq!(fs::read(&meta).unwrap(), bytes);
    }
}
#[cfg(unix)]
#[test]
fn source_history_and_lock_symlinks_are_rejected_without_reading_or_replacing_their_target() {
    use std::os::unix::fs::symlink;
    for part in ["source", "history", "lock"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rules.json");
        let target = dir.path().join("private");
        fs::write(&target, b"secret-canary").unwrap();
        let link = match part {
            "source" => path.clone(),
            "history" => suffixed(&path, ".history.json"),
            _ => suffixed(&path, ".history.lock"),
        };
        symlink(&target, &link).unwrap();
        assert!(acquire(&path, Domain::RuleSets).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"secret-canary");
    }
}
#[test]
fn child_writer_driver() {
    let Some(dir) = std::env::var_os("ZNET_FILE_HISTORY_CHILD_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let mode = std::env::var("ZNET_FILE_HISTORY_CHILD_MODE").unwrap();
    if mode == "write" {
        let path = dir.join("app-config.json");
        let mut app = app_config_store::load_or_default(&path).unwrap();
        app.local_proxy.port = 17891;
        app_config_store::save(&path, &app).unwrap();
        app.local_proxy.port = 7890;
        app_config_store::save(&path, &app).unwrap();
    } else if mode == "wait" {
        let path = dir.join("rules.json");
        let probe = OpenOptions::new()
            .read(true)
            .write(true)
            .open(suffixed(&path, ".history.lock"))
            .unwrap();
        assert!(matches!(
            probe.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        fs::write(dir.join("waiting"), b"ready").unwrap();
        let _guard = acquire(&path, Domain::RuleSets).unwrap();
        fs::write(dir.join("acquired"), b"done").unwrap();
    } else {
        let path = dir.join("rules.json");
        let mut guard = acquire(&path, Domain::RuleSets).unwrap();
        let _: AppResult<((), WriteReceipt)> =
            guard.mutate(b"after", None, Some("crashed-owner"), || {
                fs::write(&path, b"after").unwrap();
                std::process::exit(0)
            });
    }
}
fn child(dir: &Path, mode: &str) -> std::process::Command {
    let mut c = std::process::Command::new(std::env::current_exe().unwrap());
    c.args([
        "--exact",
        "services::file_history::tests::child_writer_driver",
        "--nocapture",
    ])
    .env("ZNET_FILE_HISTORY_CHILD_DIR", dir)
    .env("ZNET_FILE_HISTORY_CHILD_MODE", mode);
    c
}
#[test]
fn actual_other_process_a_b_a_advances_history_and_os_lock_serializes_contending_writer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app-config.json");
    let mut app = AppConfig::default();
    crate::services::bypass::normalize(&mut app).unwrap();
    app_config_store::save(&path, &app).unwrap();
    let before = app_config_store::current_history(&path).unwrap();
    let out = child(dir.path(), "write").output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let after = app_config_store::current_history(&path).unwrap();
    assert_eq!(after.epoch, before.epoch + 2);
    assert_eq!(app_config_store::load_or_default(&path).unwrap(), app);
    let guard = acquire(&dir.path().join("rules.json"), Domain::RuleSets).unwrap();
    let mut blocked = child(dir.path(), "wait")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !dir.path().join("waiting").exists() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(!dir.path().join("acquired").exists());
    assert!(blocked.try_wait().unwrap().is_none());
    drop(guard);
    assert!(blocked.wait().unwrap().success());
    assert!(dir.path().join("acquired").exists());
}
#[test]
fn actual_process_exit_after_file_write_keeps_unknown_until_exact_original_owner_confirms() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.json");
    fs::write(&path, b"before").unwrap();
    let before = acquire(&path, Domain::RuleSets).unwrap().token().unwrap();
    let out = child(dir.path(), "crash").output().unwrap();
    assert!(out.status.success());
    let mut guard = acquire(&path, Domain::RuleSets).unwrap();
    assert!(guard.token().is_err());
    assert!(guard.owned_token("new-operation", &before).is_err());
    let after = guard.confirm_owned("crashed-owner", &before).unwrap();
    assert!(after.epoch > before.epoch);
    assert_eq!(fs::read(&path).unwrap(), b"after");
}

#[test]
fn owned_app_receipt_binds_backup_too_and_locked_save_does_not_reenter_os_guard() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app-config.json");
    let mut app = AppConfig::default();
    crate::services::bypass::normalize(&mut app).unwrap();
    app_config_store::save(&path, &app).unwrap();
    let mut guard = acquire(&path, Domain::AppConfig).unwrap();
    let before = guard.token().unwrap();
    app.local_proxy.port = 17890;
    let receipt =
        app_config_store::save_owned_locked(&mut guard, &app, "original-operation").unwrap();
    assert_eq!(receipt.before, before);
    assert_eq!(
        guard.owned_token("original-operation", &before).unwrap(),
        receipt.after
    );
    let backup_before = fs::read(suffixed(&path, ".bak")).unwrap();
    let content = serde_json::to_vec_pretty(&app).unwrap();
    let r: AppResult<((), WriteReceipt)> = guard.mutate(
        &content,
        Some(&backup_before),
        Some("interrupted-operation"),
        || {
            fs::write(suffixed(&path, ".bak"), b"different-backup").unwrap();
            Err(unavailable())
        },
    );
    assert!(r.is_err());
    assert!(guard
        .confirm_owned("interrupted-operation", &receipt.after)
        .is_err());
}
