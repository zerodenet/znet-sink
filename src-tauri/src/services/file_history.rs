//! Cooperative file-writer history. The random store identity and monotonic
//! epoch are authority; byte digests only detect observed external changes.
//! File bodies stay in their existing stores, never in this metadata.
use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
const SCHEMA: u32 = 1;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Domain {
    AppConfig,
    RuleSets,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryToken {
    pub store_id: String,
    pub file_id: String,
    pub epoch: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    primary: Option<String>,
    backup: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WriteReceipt {
    pub before: HistoryToken,
    pub after: HistoryToken,
    pub owner_operation: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    receipt: WriteReceipt,
    expected: Snapshot,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    schema: u32,
    domain: Domain,
    token: HistoryToken,
    observed: Snapshot,
    pending: Option<Intent>,
    last_write: Option<WriteReceipt>,
}
fn unavailable() -> AppError {
    AppError {
        code: "file_history_unavailable",
        message: "本地文件的持久历史代次无法核对；未授予恢复或重写权限，请检查本地存储".into(),
        details: None,
    }
}
fn uncertain() -> AppError {
    AppError {
        code: "file_history_uncertain",
        message: "原文件写入结果未确认；保留历史屏障，不会自动重放或采用其他操作的回执".into(),
        details: None,
    }
}
fn mismatch() -> AppError {
    AppError::conflict(
        "file_history",
        "",
        "本地文件历史代次已变化；旧核对不能继续使用",
    )
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
}
fn valid_token(token: &HistoryToken) -> bool {
    valid_hex(&token.store_id, 32) && valid_hex(&token.file_id, 64) && token.epoch > 0
}
fn valid_owner(owner: &Option<String>) -> bool {
    owner.as_ref().is_none_or(|o| {
        !o.is_empty()
            && o.len() <= 128
            && o.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
    })
}
fn receipt_valid(r: &WriteReceipt) -> bool {
    valid_token(&r.before)
        && valid_token(&r.after)
        && r.before.store_id == r.after.store_id
        && r.before.file_id == r.after.file_id
        && r.before.epoch.checked_add(1) == Some(r.after.epoch)
        && valid_owner(&r.owner_operation)
}
fn snapshot_valid(s: &Snapshot) -> bool {
    [&s.primary, &s.backup]
        .into_iter()
        .all(|v| v.as_ref().is_none_or(|h| valid_hex(h, 64)))
}
fn no_follow(options: &mut OpenOptions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
}
fn regular(file: &File) -> AppResult<()> {
    let m = file.metadata().map_err(|_| unavailable())?;
    if !m.is_file() {
        return Err(unavailable());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(unavailable());
        }
    }
    Ok(())
}
fn open_read(path: &Path) -> std::io::Result<File> {
    let mut o = OpenOptions::new();
    o.read(true);
    no_follow(&mut o);
    o.open(path)
}
fn read_metadata(path: &Path) -> AppResult<Option<Vec<u8>>> {
    let file = match open_read(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(unavailable()),
    };
    regular(&file)?;
    if file.metadata().map_err(|_| unavailable())?.len() > 65536 {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| unavailable())?;
    if bytes.len() > 65536 {
        return Err(unavailable());
    }
    Ok(Some(bytes))
}
fn read_hash(path: &Path) -> AppResult<Option<String>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(unavailable()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(unavailable());
    }
    let mut input = open_read(path).map_err(|_| unavailable())?;
    regular(&input)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = input.read(&mut buffer).map_err(|_| unavailable())?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(Some(format!("{:x}", digest.finalize())))
}
fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}
fn snapshot(path: &Path, domain: Domain) -> AppResult<Snapshot> {
    Ok(Snapshot {
        primary: read_hash(path)?,
        backup: if domain == Domain::AppConfig {
            read_hash(&suffixed(path, ".bak"))?
        } else {
            None
        },
    })
}
pub(crate) struct Guard {
    path: PathBuf,
    history: PathBuf,
    _lock: File,
    meta: Metadata,
}
/// Acquire a stable OS lock before reading metadata or either source file.
/// All callers use fresh descriptors; no in-process reentrant acquisition.
pub(crate) fn acquire(path: &Path, domain: Domain) -> AppResult<Guard> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|_| unavailable())?;
    let parent = fs::canonicalize(parent).map_err(|_| unavailable())?;
    let path = parent.join(path.file_name().ok_or_else(unavailable)?);
    let lock_path = suffixed(&path, ".history.lock");
    if fs::symlink_metadata(&lock_path).is_ok_and(|m| !m.is_file() || m.file_type().is_symlink()) {
        return Err(unavailable());
    }
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    no_follow(&mut options);
    let lock = options.open(&lock_path).map_err(|_| unavailable())?;
    regular(&lock)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match lock.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) => {
                if Instant::now() >= deadline {
                    return Err(AppError {
                        code: "file_history_busy",
                        message: "本地文件正由另一宿主操作；未获取写入或恢复权限".into(),
                        details: None,
                    });
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return Err(unavailable()),
        }
    }
    let history = suffixed(&path, ".history.json");
    if fs::symlink_metadata(&history).is_ok_and(|m| !m.is_file() || m.file_type().is_symlink()) {
        return Err(unavailable());
    }
    let file_id = hash(path.as_os_str().as_encoded_bytes());
    let meta = match read_metadata(&history)? {
        Some(bytes) => {
            if bytes.len() > 65536 {
                return Err(unavailable());
            }
            let m: Metadata = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
            if m.schema != SCHEMA
                || m.domain != domain
                || !valid_token(&m.token)
                || m.token.file_id != file_id
                || !snapshot_valid(&m.observed)
            {
                return Err(unavailable());
            }
            if let Some(p) = &m.pending {
                if !receipt_valid(&p.receipt)
                    || p.receipt.after != m.token
                    || !snapshot_valid(&p.expected)
                {
                    return Err(unavailable());
                }
            }
            if let Some(r) = &m.last_write {
                if !receipt_valid(r) || r.after != m.token || m.pending.is_some() {
                    return Err(unavailable());
                }
            }
            m
        }
        None => {
            let mut entropy = [0u8; 16];
            getrandom::fill(&mut entropy).map_err(|_| unavailable())?;
            Metadata {
                schema: SCHEMA,
                domain,
                token: HistoryToken {
                    store_id: entropy.iter().map(|b| format!("{b:02x}")).collect(),
                    file_id,
                    epoch: 1,
                },
                observed: snapshot(&path, domain)?,
                pending: None,
                last_write: None,
            }
        }
    };
    let mut guard = Guard {
        path,
        history,
        _lock: lock,
        meta,
    };
    if !guard.history.exists() {
        guard.persist(&guard.meta.clone())?;
    }
    if guard.meta.pending.is_none() {
        guard.observe()?;
    }
    Ok(guard)
}
impl Guard {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn domain(&self) -> Domain {
        self.meta.domain
    }
    fn persist(&self, meta: &Metadata) -> AppResult<()> {
        let bytes = serde_json::to_vec(meta).map_err(|_| unavailable())?;
        super::atomic_file::write(&self.history, &bytes).map_err(|_| unavailable())?;
        #[cfg(unix)]
        {
            File::open(self.history.parent().ok_or_else(unavailable)?)
                .and_then(|f| f.sync_all())
                .map_err(|_| unavailable())?;
        }
        Ok(())
    }
    fn observe(&mut self) -> AppResult<()> {
        let now = snapshot(&self.path, self.meta.domain)?;
        if now != self.meta.observed {
            let mut next = self.meta.clone();
            next.token.epoch = next.token.epoch.checked_add(1).ok_or_else(unavailable)?;
            next.observed = now;
            next.last_write = None;
            self.persist(&next)?;
            self.meta = next;
        }
        Ok(())
    }
    pub(crate) fn token(&self) -> AppResult<HistoryToken> {
        if self.meta.pending.is_some() {
            return Err(uncertain());
        }
        Ok(self.meta.token.clone())
    }
    pub(crate) fn is_pending(&self) -> bool {
        self.meta.pending.is_some()
    }
    pub(crate) fn require(&self, expected: &HistoryToken) -> AppResult<()> {
        if self.token()? != *expected {
            return Err(mismatch());
        }
        Ok(())
    }
    /// Call only while holding this guard. Bodies are never copied to history.
    pub(crate) fn mutate<T>(
        &mut self,
        primary: &[u8],
        backup: Option<&[u8]>,
        owner: Option<&str>,
        write: impl FnOnce() -> AppResult<T>,
    ) -> AppResult<(T, WriteReceipt)> {
        let before = self.token()?;
        let owner = owner.map(str::to_owned);
        if !valid_owner(&owner) {
            return Err(unavailable());
        }
        let mut after = before.clone();
        after.epoch = after.epoch.checked_add(1).ok_or_else(unavailable)?;
        let receipt = WriteReceipt {
            before,
            after,
            owner_operation: owner,
        };
        let expected = Snapshot {
            primary: Some(hash(primary)),
            backup: backup
                .map(hash)
                .or_else(|| self.meta.observed.backup.clone()),
        };
        let mut pending = self.meta.clone();
        pending.token = receipt.after.clone();
        pending.pending = Some(Intent {
            receipt: receipt.clone(),
            expected: expected.clone(),
        });
        pending.last_write = None;
        self.persist(&pending)?;
        self.meta = pending;
        let result = match write() {
            Ok(v) => v,
            Err(error) => {
                // Only an observed unchanged primary AND backup can end an explicitly
                // failed attempt. A partial/unknown write keeps its pending intent.
                if snapshot(&self.path, self.meta.domain).ok().as_ref() == Some(&self.meta.observed)
                {
                    let mut clean = self.meta.clone();
                    clean.pending = None;
                    self.persist(&clean)?;
                    self.meta = clean;
                }
                return Err(error);
            }
        };
        if snapshot(&self.path, self.meta.domain)? != expected {
            return Err(uncertain());
        }
        let mut clean = self.meta.clone();
        clean.pending = None;
        clean.observed = expected;
        clean.last_write = Some(receipt.clone());
        self.persist(&clean)?;
        self.meta = clean;
        Ok((result, receipt))
    }
    /// Metadata-only reconciliation for the original, caller-authorized owner.
    /// It cannot issue a file write, reconstruct bytes, or adopt another owner.
    pub(crate) fn owned_token(
        &self,
        owner: &str,
        before: &HistoryToken,
    ) -> AppResult<HistoryToken> {
        if let Some(intent) = &self.meta.pending {
            if intent.receipt.owner_operation.as_deref() != Some(owner)
                || intent.receipt.before != *before
                || snapshot(&self.path, self.meta.domain)? != intent.expected
            {
                return Err(uncertain());
            }
            return Ok(intent.receipt.after.clone());
        }
        let receipt = self.meta.last_write.as_ref().ok_or_else(mismatch)?;
        if receipt.owner_operation.as_deref() != Some(owner)
            || receipt.before != *before
            || receipt.after != self.token()?
            || snapshot(&self.path, self.meta.domain)? != self.meta.observed
        {
            return Err(mismatch());
        }
        Ok(receipt.after.clone())
    }
    pub(crate) fn confirm_owned(
        &mut self,
        owner: &str,
        before: &HistoryToken,
    ) -> AppResult<HistoryToken> {
        let token = self.owned_token(owner, before)?;
        if let Some(intent) = &self.meta.pending {
            let mut clean = self.meta.clone();
            clean.observed = intent.expected.clone();
            clean.last_write = Some(intent.receipt.clone());
            clean.pending = None;
            self.persist(&clean)?;
            self.meta = clean;
        }
        Ok(token)
    }
}

pub(crate) fn validate_token(token: &HistoryToken) -> AppResult<()> {
    if valid_token(token) {
        Ok(())
    } else {
        Err(unavailable())
    }
}

#[cfg(test)]
mod tests;
