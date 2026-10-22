//! Read-only metadata evidence. In particular do not call journal::acquire:
//! it intentionally creates/initializes storage, while inspection never may.
use super::*;
use sha2::{Digest, Sha256};
use std::io;

pub(crate) fn report(code: &'static str, evidence: PacketLedgerEvidence) -> PacketLedgerInspection {
    let (state, summary) = match code {
        "ready" => (
            "ready",
            "持久账本结构与本次协同读取已核对；这不表示会话已关闭，也不授予控制权限",
        ),
        "not-initialized" => (
            "not-initialized",
            "账本尚未初始化；只读检查未创建任何文件，下次控制仍须完成真实持久预留",
        ),
        "initialization-pending" => (
            "not-initialized",
            "账本初始化标记尚未完成；本次只读检查不会补写标记或恢复权限",
        ),
        "missing-initialized" => (
            "blocked",
            "已初始化的账本丢失，无法重建原未知目标集合；保留屏障，不会新建空账本",
        ),
        "invalid-json" => (
            "blocked",
            "账本正文不是可读取的 JSON；无法安全还原原目标集合，不会清空或重放",
        ),
        "untrusted-structure" => (
            "blocked",
            "账本字段、记录身份或数量不符合受支持契约；不会采用部分记录或删除原证据",
        ),
        "unsupported-schema" => (
            "blocked",
            "账本 schema 不受当前客户端支持；保留原证据并暂停新关闭",
        ),
        "oversized" => (
            "blocked",
            "账本超过 1 MiB 读取预算；未读取超限正文或计算不完整摘要",
        ),
        "unsafe-file" => (
            "blocked",
            "账本或标记不是可安全读取的普通文件；不会跟随链接、替换或修复它",
        ),
        "invalid-marker" => ("blocked", "初始化标记不可信；无法把缺失或旧材料视为空账本"),
        "coordination-missing" => (
            "blocked",
            "账本存在但协同标记缺失，无法核对稳定快照；只保留可读文件元数据",
        ),
        "lock-busy" => (
            "blocked",
            "另一读写者占用账本锁；未读取正文、未等待或改变原记录，可稍后重新检查",
        ),
        "lock-unsupported" => (
            "blocked",
            "文件系统不支持所需协同锁；不会降级为无锁读取或控制",
        ),
        "permission-denied" => (
            "blocked",
            "当前权限不足以协同读取账本；不回显系统错误或敏感路径",
        ),
        "changed-during-inspection" => (
            "blocked",
            "检查期间文件元数据变化，摘要和结构结果未交付；原记录保持不变",
        ),
        "publication-unconfirmed" => (
            "blocked",
            "当前进程仍保留未确认存盘屏障；文件可读不等于原存盘或关闭已确认",
        ),
        "entropy-unavailable" => (
            "blocked",
            "安全进程身份不可用；只读证据可查看，控制继续拒绝",
        ),
        "capacity-reached" => (
            "blocked",
            "已有 128 个未协调物理目标；只读检查不会丢弃记录以释放预算",
        ),
        "state-unavailable" => (
            "blocked",
            "进程内屏障状态不可可靠访问；保留已有屏障，控制继续拒绝",
        ),
        _ => (
            "blocked",
            "账本读取发生 IO 故障；保留原文件和屏障，不回显系统错误或路径",
        ),
    };
    PacketLedgerInspection {
        host_session_id: None,
        sampled_at_unix_ms: crate::services::common::now_unix_ms().to_string(),
        state,
        reason_code: code,
        summary,
        read_only: true,
        barriers_retained: true,
        new_close_blocked: state == "blocked",
        in_memory_barrier_count: None,
        publication_unconfirmed: false,
        evidence,
    }
}
fn io_code(error: &io::Error) -> &'static str {
    if error.kind() == io::ErrorKind::PermissionDenied {
        "permission-denied"
    } else {
        "io-unavailable"
    }
}
fn metadata(path: &Path) -> Result<Option<fs::Metadata>, &'static str> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ok(Some(meta)),
        Ok(_) => Err("unsafe-file"),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_code(&error)),
    }
}
fn same_file(before: &fs::Metadata, after: &fs::Metadata) -> bool {
    let same = before.len() == after.len()
        && before.modified().ok().is_some()
        && before.modified().ok() == after.modified().ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        same && before.dev() == after.dev() && before.ino() == after.ino()
    }
    #[cfg(not(unix))]
    {
        same && before.created().ok() == after.created().ok()
    }
}
fn open(path: &Path, writable: bool) -> Result<fs::File, &'static str> {
    let mut options = fs::OpenOptions::new();
    options.read(true).write(writable); // No create/truncate, including marker.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    }
    options.open(path).map_err(|error| io_code(&error))
}
fn lock_error(error: fs::TryLockError) -> (&'static str, &'static str) {
    match error {
        fs::TryLockError::WouldBlock => ("lock-busy", "busy"),
        fs::TryLockError::Error(error) if error.kind() == io::ErrorKind::Unsupported => {
            ("lock-unsupported", "unsupported")
        }
        fs::TryLockError::Error(error) => (io_code(&error), "unavailable"),
    }
}
/// Exact file identity/size/mtime observations around a bounded read. The
/// shared marker lock excludes cooperating writers; no malicious local
/// rollback or filesystem snapshot guarantee is inferred from metadata.
fn bytes(path: &Path, limit: usize) -> Result<Vec<u8>, &'static str> {
    bytes_observed(path, limit, || {})
}
fn bytes_observed(
    path: &Path,
    limit: usize,
    after_read: impl FnOnce(),
) -> Result<Vec<u8>, &'static str> {
    let before = metadata(path)?.ok_or("changed-during-inspection")?;
    if before.len() > limit as u64 {
        return Err("oversized");
    }
    let file = open(path, false)?;
    bytes_opened(path, &file, &before, limit, after_read)
}
fn bytes_opened(
    path: &Path,
    file: &fs::File,
    before: &fs::Metadata,
    limit: usize,
    after_read: impl FnOnce(),
) -> Result<Vec<u8>, &'static str> {
    if before.len() > limit as u64 {
        return Err("oversized");
    }
    let opened = file.metadata().map_err(|error| io_code(&error))?;
    if !opened.is_file() || !same_file(before, &opened) {
        return Err("changed-during-inspection");
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_code(&error))?;
    if bytes.len() > limit {
        return Err("oversized");
    }
    after_read();
    let after = file.metadata().map_err(|error| io_code(&error))?;
    let current = metadata(path)?.ok_or("changed-during-inspection")?;
    if !same_file(&opened, &after)
        || !same_file(&after, &current)
        || after.len() != bytes.len() as u64
    {
        return Err("changed-during-inspection");
    }
    Ok(bytes)
}
fn classify(
    path: &Path,
    evidence: &mut PacketLedgerEvidence,
) -> Result<&'static str, &'static str> {
    let journal_meta = metadata(path)?;
    evidence.journal_present = Some(journal_meta.is_some());
    evidence.journal_bytes = journal_meta.as_ref().map(|meta| meta.len().to_string());
    let marker_path = lock_path(path);
    let marker_meta = metadata(&marker_path)?;
    evidence.marker_present = Some(marker_meta.is_some());
    let Some(marker_meta) = marker_meta else {
        evidence.marker_state = "absent";
        evidence.lock_state = "not-present";
        return Ok(if journal_meta.is_none() {
            "not-initialized"
        } else {
            "coordination-missing"
        });
    };
    let marker_file = open(&marker_path, true)?;
    if !marker_file
        .metadata()
        .is_ok_and(|meta| meta.is_file() && same_file(&marker_meta, &meta))
    {
        return Err("changed-during-inspection");
    }
    if let Err(error) = marker_file.try_lock() {
        let (code, state) = lock_error(error);
        evidence.lock_state = state;
        return Err(code);
    }
    evidence.lock_state = "acquired";
    // Re-read under lock, not metadata cached before waiting for ownership.
    // Windows byte-range locks belong to this handle; a second handle may
    // not read its locked bytes even in the same process. Read the exact
    // descriptor already owning the marker lock, without reopening it.
    let marker =
        bytes_opened(&marker_path, &marker_file, &marker_meta, 64, || {}).map_err(|code| {
            if code == "oversized" {
                "invalid-marker"
            } else {
                code
            }
        })?;
    evidence.marker_state = match marker.as_slice() {
        b"" => "empty",
        b"packet-close-ledger-v1\n" => "initialized",
        _ => "untrusted",
    };
    if evidence.marker_state == "untrusted" {
        return Err("invalid-marker");
    }
    let journal_meta = metadata(path)?;
    evidence.journal_present = Some(journal_meta.is_some());
    evidence.journal_bytes = journal_meta.as_ref().map(|meta| meta.len().to_string());
    if journal_meta.is_none() {
        return Ok(if evidence.marker_state == "initialized" {
            "missing-initialized"
        } else {
            "not-initialized"
        });
    }
    let raw = bytes(path, BYTE_LIMIT)?;
    // Also ensure the lock path has not been replaced while its old handle
    // remains locked. Inspection never adopts a second uncoordinated lock.
    if !metadata(&marker_path)?.is_some_and(|current| same_file(&marker_meta, &current)) {
        return Err("changed-during-inspection");
    }
    evidence.snapshot_stable = true;
    evidence.journal_sha256 = Some(format!("{:x}", Sha256::digest(&raw)));
    let value: serde_json::Value = serde_json::from_slice(&raw).map_err(|_| "invalid-json")?;
    let schema = value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        .ok_or("untrusted-structure")?;
    evidence.schema_version = u32::try_from(schema).ok();
    if schema != 1 {
        return Err("unsupported-schema");
    }
    let envelope: Envelope = serde_json::from_value(value).map_err(|_| "untrusted-structure")?;
    validate(&envelope.records).map_err(|_| "untrusted-structure")?;
    evidence.verified_record_count = Some(envelope.records.len());
    Ok(if evidence.marker_state == "initialized" {
        "ready"
    } else {
        "initialization-pending"
    })
}
pub(crate) fn inspect(path: &Path) -> PacketLedgerInspection {
    let mut evidence = PacketLedgerEvidence {
        marker_state: "not-read",
        lock_state: "not-checked",
        ..Default::default()
    };
    let code = match classify(path, &mut evidence) {
        Ok(code) | Err(code) => code,
    };
    report(code, evidence)
}

#[cfg(test)]
#[path = "inspection_tests.rs"]
mod tests;
