//! Durable ambiguity barrier. Contains identities/actions only, no config
//! bodies, subscription URLs, proxy secrets, or plugin material.
use super::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Read, path::Path};
const MAX_BYTES: u64 = 1024 * 1024;
const MAX_ENTRIES: usize = 128;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    guard: String,
    action: RemovalAction,
    runtime: RemovalRuntime,
}
fn path() -> AppResult<std::path::PathBuf> {
    Ok(crate::services::data_dir()?.join("profile-removal-pending-v1.json"))
}
fn invalid() -> AppError {
    AppError::invalid_argument(
        "配置删除恢复记录损坏或超出 1 MiB / 128 项预算；请诊断本地存储，保留原恢复记录",
    )
}
fn validate(entries: &BTreeMap<String, Entry>) -> AppResult<()> {
    if entries.len() > MAX_ENTRIES || entries.iter().any(|(id, e)| {
        id.is_empty() || id.len() > 512 || e.guard.len() != 64 || !e.guard.bytes().all(|b| b.is_ascii_hexdigit())
        || matches!(&e.action, RemovalAction::Replace { replacement_id } if replacement_id.is_empty() || replacement_id.len() > 512)
        || !["offline", "running", "external", "unknown", "platform_unknown"].contains(&e.runtime.state.as_str())
        || e.runtime.core_instance_id.as_ref().is_some_and(|id| id.is_empty() || id.len() > 512)
        || e.runtime.config_revision.as_ref().is_some_and(|v| v.parse::<u64>().ok().map(|n| n.to_string()).as_ref() != Some(v))
    }) { return Err(invalid()); }
    Ok(())
}
fn load_at(p: &Path) -> AppResult<BTreeMap<String, Entry>> {
    let file = match std::fs::File::open(p) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(_) => return Err(AppError::internal("配置删除恢复记录读取失败")),
    };
    if file
        .metadata()
        .map_err(|_| AppError::internal("配置删除恢复记录信息读取失败"))?
        .len()
        > MAX_BYTES
    {
        return Err(invalid());
    }
    // Recheck actual bytes after metadata: a concurrent rewrite must not turn
    // a bounded recovery read into an unbounded allocation.
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AppError::internal("配置删除恢复记录读取中断"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid());
    }
    let entries = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    validate(&entries)?;
    Ok(entries)
}
fn load() -> AppResult<BTreeMap<String, Entry>> {
    load_at(&path()?)
}
fn save_at(p: &Path, entries: &BTreeMap<String, Entry>) -> AppResult<()> {
    validate(entries)?;
    let bytes =
        serde_json::to_vec(entries).map_err(|_| AppError::internal("配置删除恢复记录编码失败"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid());
    }
    crate::services::atomic_file::write(p, &bytes)
        .map_err(|_| AppError::internal("配置删除恢复记录保存失败；未开始新删除"))
}
fn save(entries: &BTreeMap<String, Entry>) -> AppResult<()> {
    save_at(&path()?, entries)
}
pub(super) fn pending(id: &str) -> AppResult<bool> {
    Ok(load()?.contains_key(id))
}
pub(super) fn begin(input: &RemovalInput, runtime: &RemovalRuntime) -> AppResult<()> {
    let mut entries = load()?;
    if entries.contains_key(&input.target_id) {
        return Err(AppError::conflict(
            "profile_removal",
            &input.target_id,
            "原删除仍待核对，不能重放",
        ));
    }
    if entries.len() >= MAX_ENTRIES {
        return Err(invalid());
    }
    entries.insert(
        input.target_id.clone(),
        Entry {
            guard: input.guard.clone(),
            action: input.action.clone(),
            runtime: runtime.clone(),
        },
    );
    save(&entries)
}
pub(super) fn clear(id: &str) -> AppResult<()> {
    let mut entries = load()?;
    entries.remove(id);
    save(&entries)
}
pub(super) fn unchanged_inactive(id: &str, guard: &str) -> AppResult<bool> {
    Ok(load()?.get(id).is_some_and(|entry| {
        matches!(entry.action, RemovalAction::Inactive {}) && entry.guard == guard
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn entry() -> Entry {
        Entry {
            guard: "a".repeat(64),
            action: RemovalAction::Inactive {},
            runtime: RemovalRuntime {
                state: "platform_unknown".into(),
                core_instance_id: None,
                config_revision: None,
                managed_pid: None,
            },
        }
    }
    #[test]
    fn recovery_record_survives_reload_and_contains_only_identity_data() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("pending.json");
        save_at(&p, &BTreeMap::from([("target".into(), entry())])).unwrap();
        assert_eq!(load_at(&p).unwrap()["target"].guard, "a".repeat(64));
        let payload = String::from_utf8(std::fs::read(&p).unwrap()).unwrap();
        assert!(!payload.contains("url"));
        assert!(!payload.contains("content"));
        assert!(!payload.contains("secret"));
    }
    #[test]
    fn oversized_corrupt_and_excess_identity_records_are_refused_and_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("pending.json");
        for bytes in [
            b"{bad json".to_vec(),
            vec![b' '; (MAX_BYTES + 1) as usize],
            serde_json::to_vec(
                &(0..129)
                    .map(|i| (i.to_string(), entry()))
                    .collect::<BTreeMap<_, _>>(),
            )
            .unwrap(),
        ] {
            std::fs::write(&p, &bytes).unwrap();
            assert!(load_at(&p).is_err());
            assert_eq!(std::fs::read(&p).unwrap(), bytes);
        }
    }
    #[test]
    fn rejected_publication_keeps_previous_recovery_generation() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("pending.json");
        save_at(&p, &BTreeMap::from([("target".into(), entry())])).unwrap();
        let previous = std::fs::read(&p).unwrap();
        let excess = (0..129)
            .map(|i| (i.to_string(), entry()))
            .collect::<BTreeMap<_, _>>();
        assert!(save_at(&p, &excess).is_err());
        assert_eq!(std::fs::read(&p).unwrap(), previous);
    }
}
