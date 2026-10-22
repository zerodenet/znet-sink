use super::*;
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    process::{Command, Stdio},
    sync::{Arc, Barrier},
};

fn target(id: usize) -> PacketTarget {
    PacketTarget {
        profile_id: Some("p1".into()),
        client_scope_generation: 1,
        host_session_id: "current-host".into(),
        close_operation_id: None,
        route_id: format!("packet-{id}"),
        core_instance_id: "original-core".into(),
        config_revision: u64::MAX.to_string(),
    }
}
fn child(path: &Path, mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "services::packet_routes::ledger::tests::journal_child_driver",
            "--nocapture",
        ])
        .env("ZNET_PACKET_TEST_CHILD", mode)
        .env("ZNET_PACKET_TEST_JOURNAL", path);
    command
}
#[test]
fn journal_child_driver() {
    let Ok(mode) = std::env::var("ZNET_PACKET_TEST_CHILD") else {
        return;
    };
    let path = PathBuf::from(std::env::var_os("ZNET_PACKET_TEST_JOURNAL").unwrap());
    match mode.as_str() {
        "inspect" => {
            let ledger = CloseLedger::at(&path);
            let view = ledger.view();
            assert_eq!(view.recovery.state, "ready");
            assert_eq!(view.records.len(), 1);
            assert!(view.records[0].recovered);
            assert_eq!(view.recovery.recovered_count, Some(1));
            assert!(ledger.reserve(&target(1)).is_err());
            println!("packet-child-recovered");
        }
        "reserve" => {
            CloseLedger::at(&path).reserve(&target(2)).unwrap();
        }
        "hold-lock" => {
            let _lock = journal::acquire(&path).unwrap();
            println!("packet-child-lock-held");
            std::io::stdout().flush().unwrap();
            let mut release = [0u8; 1];
            std::io::stdin().read_exact(&mut release).unwrap();
        }
        _ => panic!("unknown test mode"),
    }
}
#[test]
fn real_child_reopens_unknown_without_replay_and_writers_merge() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("journal.json");
    let ledger = CloseLedger::at(&path);
    let original = ledger.reserve(&target(1)).unwrap();
    let output = child(&path, "inspect").output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("packet-child-recovered"));
    let output = child(&path, "reserve").output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    ledger.reserve(&target(3)).unwrap();
    let records = journal::load(&path).unwrap();
    assert_eq!(records.len(), 3);
    assert!(records
        .iter()
        .any(|row| row.operation_id == original.record.operation_id));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let text = fs::read_to_string(path).unwrap();
    for forbidden in [
        "sourceAddress",
        "destination",
        "payload",
        "privateKey",
        "hostSessionId",
    ] {
        assert!(!text.contains(forbidden));
    }
}
#[test]
fn real_child_lock_contention_blocks_writes_without_erasing_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("journal.json");
    let ledger = CloseLedger::at(&path);
    ledger.reserve(&target(1)).unwrap();
    let mut worker = child(&path, "hold-lock")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(worker.stdout.take().unwrap());
    let mut seen = false;
    for line in reader.by_ref().lines() {
        if line.unwrap().contains("packet-child-lock-held") {
            seen = true;
            break;
        }
    }
    assert!(seen);
    assert!(ledger.reserve(&target(2)).is_err());
    let blocked = ledger.view();
    assert_eq!(blocked.recovery.state, "blocked");
    assert_eq!(blocked.records.len(), 1);
    worker.stdin.take().unwrap().write_all(b"x").unwrap();
    assert!(worker.wait().unwrap().success());
    ledger.reserve(&target(2)).unwrap();
    assert_eq!(journal::load(&path).unwrap().len(), 2);
}
#[test]
fn separate_openers_concurrently_reserve_physical_target_once() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("journal.json");
    assert_eq!(CloseLedger::at(&path).view().recovery.state, "ready");
    let barrier = Arc::new(Barrier::new(2));
    let workers = (0..2)
        .map(|_| {
            let barrier = barrier.clone();
            let path = path.clone();
            std::thread::spawn(move || {
                let ledger = CloseLedger::at(&path);
                barrier.wait();
                ledger.reserve(&target(1)).is_ok()
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .filter(|success| *success)
            .count(),
        1
    );
    assert_eq!(journal::load(&path).unwrap().len(), 1);
}
#[test]
fn corrupt_oversized_and_unknown_fields_are_preserved_fail_closed() {
    for bytes in [
        b"broken".to_vec(),
        vec![b'x'; journal::BYTE_LIMIT + 1],
        br#"{"schemaVersion":1,"records":[],"payload":"secret"}"#.to_vec(),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("journal.json");
        fs::write(&path, &bytes).unwrap();
        let ledger = CloseLedger::at(&path);
        assert_eq!(ledger.view().recovery.state, "blocked");
        assert!(ledger.reserve(&target(1)).is_err());
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}
#[test]
fn disappeared_initialized_journal_never_becomes_empty_again() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("journal.json");
    let ledger = CloseLedger::at(&path);
    ledger.reserve(&target(1)).unwrap();
    fs::remove_file(&path).unwrap();
    assert_eq!(ledger.view().records.len(), 1);
    assert_eq!(ledger.view().recovery.state, "blocked");
    let fresh = CloseLedger::at(&path);
    assert!(fresh.reserve(&target(2)).is_err());
    assert!(!path.exists());
}
#[test]
fn failed_reserve_publication_retains_memory_and_blocks_new_effects() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("journal.json");
    let ledger = CloseLedger::at(&path);
    ledger.reserve(&target(1)).unwrap();
    ledger.fail_next_publication();
    assert!(ledger.reserve(&target(2)).is_err());
    let view = ledger.view();
    assert_eq!(view.recovery.state, "blocked");
    assert_eq!(view.records.len(), 2);
    assert!(ledger.reserve(&target(3)).is_err());
    assert_eq!(journal::load(&path).unwrap().len(), 1);
    // Original fixed-target authority may safely complete a barrier once storage works.
    let reservation = ledger.observe(&target(2)).unwrap().unwrap();
    ledger.complete(&reservation).unwrap();
    assert_eq!(ledger.view().recovery.state, "ready");
    assert_eq!(journal::load(&path).unwrap().len(), 1);
}
#[test]
fn failed_completion_keeps_durable_barrier_and_late_token_cannot_release_new_operation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("journal.json");
    let ledger = CloseLedger::at(&path);
    let first = ledger.reserve(&target(1)).unwrap();
    ledger.fail_next_publication();
    assert!(ledger.complete(&first).is_err());
    assert_eq!(journal::load(&path).unwrap().len(), 1);
    let restored = CloseLedger::at(&path);
    assert!(restored.reserve(&target(1)).is_err());
    restored.complete(&first).unwrap();
    let newer = restored.reserve(&target(1)).unwrap();
    assert!(restored.complete(&first).is_err());
    let mut stale = target(1);
    stale.close_operation_id = Some(first.record.operation_id);
    assert!(restored.observe(&stale).is_err());
    assert_eq!(
        restored.view().records[0].operation_id,
        newer.record.operation_id
    );
}
#[test]
fn records_never_age_out_or_clear_for_configuration_aba() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("journal.json");
    let ledger = CloseLedger::at(&path);
    let mut record = ledger.reserve(&target(1)).unwrap().record;
    record.reserved_at_unix_ms = 1;
    journal::save(&path, &[record.clone()]).unwrap();
    let restored = CloseLedger::at(&path);
    let mut aba = target(1);
    aba.client_scope_generation = 999;
    aba.profile_id = Some("after-aba".into());
    aba.host_session_id = "new-host".into();
    assert!(restored.reserve(&aba).is_err());
    assert_eq!(restored.view().records[0].operation_id, record.operation_id);
}
#[test]
fn actual_file_failure_and_symlinks_never_replace_authoritative_journal() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("journal.json");
    let ledger = CloseLedger::at(&path);
    let reservation = ledger.reserve(&target(1)).unwrap();
    let saved = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(ledger.complete(&reservation).is_err());
    assert_eq!(ledger.view().records.len(), 1);
    fs::remove_dir(&path).unwrap();
    fs::write(&path, saved).unwrap();
    assert_eq!(ledger.view().records.len(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link = directory.path().join("alias.json");
        symlink(&path, &link).unwrap();
        assert!(CloseLedger::at(&link).reserve(&target(2)).is_err());
        assert_eq!(journal::load(&path).unwrap().len(), 1);
    }
}
#[test]
fn bounded_record_budget_and_duplicate_identities_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("journal.json");
    let ledger = CloseLedger::at(&path);
    let row = ledger.reserve(&target(1)).unwrap().record;
    assert!(journal::validate(&[row.clone(), row.clone()]).is_err());
    let mut wrong = row.clone();
    wrong.origin_client_scope_generation = 0;
    assert!(journal::validate(&[wrong]).is_err());
    let mut wrong = row.clone();
    wrong.target.config_revision = "01".into();
    assert!(journal::validate(&[wrong]).is_err());
    let mut payload = serde_json::to_value(&row).unwrap();
    payload["target"]["rawPayload"] = json!("no");
    assert!(serde_json::from_value::<Record>(payload).is_err());
    let mut rows = Vec::new();
    for index in 0..=CAPACITY {
        let mut item = row.clone();
        item.target.route_id = format!("packet-{index}");
        item.operation_id = format!("packet-close-{index:032x}");
        rows.push(item);
    }
    assert!(journal::validate(&rows).is_err());
    assert_eq!(journal::load(&path).unwrap().len(), 1);
}

#[cfg(unix)]
#[test]
fn actual_denied_atomic_publication_preserves_previous_file_and_barrier() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("journal.json");
    let ledger = CloseLedger::at(&path);
    let reservation = ledger.reserve(&target(1)).unwrap();
    let before = fs::read(&path).unwrap();
    // On privileged runners use an actually read-only OS directory rather
    // than assuming chmod can deny root. This still exercises atomic_file IO.
    if unsafe { libc::geteuid() } == 0 {
        #[cfg(target_os = "linux")]
        assert!(
            journal::save(Path::new("/proc/self/znet-packet-atomic-write-denied"), &[]).is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(ledger.view().records.len(), 1);
    } else {
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o500)).unwrap();
        let result = ledger.complete(&reservation);
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(ledger.view().recovery.state, "blocked");
        assert_eq!(ledger.view().records.len(), 1);
        assert!(ledger.reserve(&target(2)).is_err());
        ledger.complete(&reservation).unwrap();
        assert!(ledger.view().records.is_empty());
    }
}
