//! Private, bounded metadata only. No packet bytes, addresses, log text or config.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
#[path = "inspection.rs"]
mod inspection;
pub(super) use inspection::{inspect, report};

pub(super) const BYTE_LIMIT: usize = 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Record {
    pub target: PacketPhysicalTarget,
    pub operation_id: String,
    pub origin_process_id: String,
    pub origin_profile_id: Option<String>,
    pub origin_client_scope_generation: u64,
    pub reserved_at_unix_ms: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    schema_version: u32,
    records: Vec<Record>,
}
pub(super) struct JournalLock {
    pub records: Vec<Record>,
    _file: fs::File,
}
pub(super) fn unavailable() -> AppError {
    AppError { code: "packet_ledger_unavailable", message: "Packet 关闭恢复账本损坏、不可读、繁忙或存盘未确认；保留原记录并暂停新关闭，读取列表仍可用".into(), details: None }
}
fn token(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|suffix| {
        suffix.len() == 32
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}
pub(super) fn validate(records: &[Record]) -> AppResult<()> {
    let mut physical = std::collections::BTreeSet::new();
    let mut operations = std::collections::BTreeSet::new();
    if records.len() > ledger::CAPACITY {
        return Err(unavailable());
    }
    for row in records {
        let target = &row.target;
        if target.route_id.trim().is_empty()
            || target.route_id.len() > 1024
            || target.core_instance_id.trim().is_empty()
            || target.core_instance_id.len() > 1024
            || target
                .config_revision
                .parse::<u64>()
                .ok()
                .map(|value| value.to_string())
                .as_ref()
                != Some(&target.config_revision)
            || !token(&row.operation_id, "packet-close-")
            || !token(&row.origin_process_id, "host-")
            || row
                .origin_profile_id
                .as_ref()
                .is_some_and(|id| id.is_empty() || id.len() > 512)
            || row.origin_client_scope_generation == 0
            || row.origin_client_scope_generation > 9_007_199_254_740_991
            || row.reserved_at_unix_ms == 0
            || !physical.insert(target)
            || !operations.insert(&row.operation_id)
        {
            return Err(unavailable());
        }
    }
    Ok(())
}
fn regular(path: &Path) -> AppResult<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ok(Some(meta)),
        _ => Err(unavailable()),
    }
}
fn lock_path(path: &Path) -> PathBuf {
    path.with_extension("lock")
}
pub(super) fn acquire(path: &Path) -> AppResult<JournalLock> {
    let parent = path.parent().ok_or_else(unavailable)?;
    fs::create_dir_all(parent).map_err(|_| unavailable())?;
    let lock_path = lock_path(path);
    regular(&lock_path)?;
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut file = options.open(&lock_path).map_err(|_| unavailable())?;
    if !file.metadata().map_err(|_| unavailable())?.is_file() {
        return Err(unavailable());
    }
    // Unsupported OS locking is a hard denial, never a silent unlocked path.
    file.try_lock().map_err(|_| unavailable())?;
    const MARKER: &[u8] = b"packet-close-ledger-v1\n";
    if file.metadata().map_err(|_| unavailable())?.len() > 64 {
        return Err(unavailable());
    }
    let mut marker = Vec::new();
    (&file)
        .take(65)
        .read_to_end(&mut marker)
        .map_err(|_| unavailable())?;
    if !marker.is_empty() && marker != MARKER {
        return Err(unavailable());
    }
    let exists = regular(path)?.is_some();
    if !marker.is_empty() && !exists {
        return Err(unavailable());
    }
    let records = load(path)?;
    if !exists {
        save(path, &[])?;
    }
    if marker.is_empty() {
        file.write_all(MARKER)
            .and_then(|()| file.sync_all())
            .map_err(|_| unavailable())?;
        #[cfg(unix)]
        fs::File::open(parent)
            .and_then(|dir| dir.sync_all())
            .map_err(|_| unavailable())?;
    }
    Ok(JournalLock {
        records,
        _file: file,
    })
}
pub(super) fn load(path: &Path) -> AppResult<Vec<Record>> {
    let Some(meta) = regular(path)? else {
        return Ok(Vec::new());
    };
    if meta.len() > BYTE_LIMIT as u64 {
        return Err(unavailable());
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options.open(path).map_err(|_| unavailable())?;
    let meta = file.metadata().map_err(|_| unavailable())?;
    if !meta.is_file() || meta.len() > BYTE_LIMIT as u64 {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    file.take(BYTE_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unavailable())?;
    if bytes.len() > BYTE_LIMIT {
        return Err(unavailable());
    }
    let envelope: Envelope = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    if envelope.schema_version != 1 {
        return Err(unavailable());
    }
    validate(&envelope.records)?;
    Ok(envelope.records)
}
pub(super) fn save(path: &Path, records: &[Record]) -> AppResult<()> {
    validate(records)?;
    regular(path)?;
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: 1,
        records: records.to_vec(),
    })
    .map_err(|_| unavailable())?;
    if bytes.len() > BYTE_LIMIT {
        return Err(unavailable());
    }
    crate::services::atomic_file::write(path, &bytes).map_err(|_| unavailable())?;
    #[cfg(unix)]
    fs::File::open(path.parent().ok_or_else(unavailable)?)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| unavailable())?;
    Ok(())
}
