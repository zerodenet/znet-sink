use super::*;
use crate::models::rule_set::ZrsArtifact;
use serde_json::json;

fn fixture() -> (tempfile::TempDir, RuleSetProfile, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let rules = zero_rule::protocol::decode_json(
        &serde_json::to_vec(
            &json!({"version":1,"rules":[{"type":"domain_exact","value":"example.com"}]}),
        )
        .unwrap(),
    )
    .unwrap();
    let (compiled, _) = zero_rule::RuleSetCompiler.compile(rules).unwrap();
    let bytes = zero_rule::zrs::encode(&compiled).unwrap();
    let metadata =
        zero_rule::zrs::verify(&bytes, zero_rule::zrs::VerifyMode::FullChecksum).unwrap();
    let path = dir
        .path()
        .join("rule-artifacts")
        .join("owned")
        .join(format!("1-{:08x}.zrs", metadata.body_checksum));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let mut p:RuleSetProfile=serde_json::from_value(json!({"id":"owned","name":"ordinary","enabled":true,"semanticIr":{"version":1,"rules":[]},"updatedAtUnixMs":1})).unwrap();
    p.artifact = Some(ZrsArtifact {
        path: path.to_string_lossy().into_owned(),
        major_version: metadata.major_version,
        minor_version: metadata.minor_version,
        checksum: metadata.body_checksum,
        file_size: metadata.file_size,
        entry_count: metadata.entry_count(),
        built_at_unix_ms: 1,
    });
    (dir, p, bytes)
}
#[test]
fn actual_owned_zrs_bytes_and_every_declared_metadata_field_are_verified() {
    let (dir, p, bytes) = fixture();
    assert_eq!(
        verify(dir.path(), &p).unwrap(),
        format!("{:x}", Sha256::digest(&bytes))
    );
}
#[test]
fn missing_corrupt_and_truncated_owned_files_do_not_provide_a_proof_or_leak_path() {
    for mode in 0..3 {
        let (dir, p, mut bytes) = fixture();
        let path = &p.artifact.as_ref().unwrap().path;
        match mode {
            0 => std::fs::remove_file(path).unwrap(),
            1 => {
                let last = bytes.len() - 1;
                bytes[last] ^= 1;
                std::fs::write(path, bytes).unwrap();
            }
            _ => std::fs::write(path, &bytes[..8]).unwrap(),
        };
        let error = verify(dir.path(), &p).unwrap_err();
        assert_eq!(error.code, "rule_migration_artifact_unavailable");
        assert!(error.details.is_none());
        assert!(!error
            .message
            .contains(&dir.path().to_string_lossy().to_string()));
    }
}
#[test]
fn another_owner_or_external_or_traversing_path_never_grants_artifact_access() {
    for mode in 0..5 {
        let (dir, mut p, bytes) = fixture();
        match mode {
            0 => p.id = "another".into(),
            1 => {
                let outside = dir.path().join("outside.zrs");
                std::fs::write(&outside, &bytes).unwrap();
                p.artifact.as_mut().unwrap().path = outside.to_string_lossy().into_owned();
            }
            2 => p.id = "../owned".into(),
            3 => p.id = dir.path().to_string_lossy().into_owned(),
            _ => p.id = "a/b".into(),
        };
        assert!(verify(dir.path(), &p).is_err());
    }
}
#[test]
fn declaration_size_version_checksum_count_and_generation_mismatch_are_rejected() {
    for mode in 0..6 {
        let (dir, mut p, _) = fixture();
        let a = p.artifact.as_mut().unwrap();
        match mode {
            0 => a.file_size += 1,
            1 => a.major_version += 1,
            2 => a.minor_version += 1,
            3 => a.checksum ^= 1,
            4 => a.entry_count += 1,
            _ => a.built_at_unix_ms += 1,
        };
        assert!(verify(dir.path(), &p).is_err());
    }
}
#[test]
fn u64_size_and_actual_sparse_file_budget_are_checked_before_reading() {
    for declared in [MAX_BYTES + 1, u64::MAX] {
        let (dir, mut p, _) = fixture();
        p.artifact.as_mut().unwrap().file_size = declared;
        assert!(verify(dir.path(), &p).is_err());
    }
    let (dir, p, _) = fixture();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&p.artifact.as_ref().unwrap().path)
        .unwrap()
        .set_len(MAX_BYTES + 1)
        .unwrap();
    assert!(verify(dir.path(), &p).is_err());
}
#[cfg(unix)]
#[test]
fn fifo_and_directory_in_place_of_owned_artifact_are_rejected_without_blocking() {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    for fifo in [false, true] {
        let (dir, p, _) = fixture();
        let path = Path::new(&p.artifact.as_ref().unwrap().path);
        std::fs::remove_file(path).unwrap();
        if fifo {
            let path = CString::new(path.as_os_str().as_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        } else {
            std::fs::create_dir(path).unwrap();
        }
        assert!(verify(dir.path(), &p).is_err());
    }
}
#[cfg(unix)]
#[test]
fn file_parent_and_root_symlinks_are_rejected_even_when_they_resolve_to_valid_zrs() {
    use std::os::unix::fs::symlink;
    for mode in 0..3 {
        let (dir, p, bytes) = fixture();
        let artifact = Path::new(&p.artifact.as_ref().unwrap().path);
        match mode {
            0 => {
                let outside = dir.path().join("outside.zrs");
                std::fs::write(&outside, &bytes).unwrap();
                std::fs::remove_file(artifact).unwrap();
                symlink(outside, artifact).unwrap();
                assert!(verify(dir.path(), &p).is_err());
            }
            1 => {
                let owner = artifact.parent().unwrap();
                let other = dir.path().join("other");
                std::fs::rename(owner, &other).unwrap();
                symlink(other, owner).unwrap();
                assert!(verify(dir.path(), &p).is_err());
            }
            _ => {
                let root = tempfile::tempdir().unwrap();
                let link = root.path().join("linked");
                symlink(dir.path(), &link).unwrap();
                let mut p = p.clone();
                p.artifact.as_mut().unwrap().path = link
                    .join("rule-artifacts")
                    .join("owned")
                    .join(artifact.file_name().unwrap())
                    .to_string_lossy()
                    .into_owned();
                assert!(verify(&link, &p).is_err());
            }
        }
    }
}
