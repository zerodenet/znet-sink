//! Query-only, bounded workspace ledger diagnostics. Does not call the normal
//! database opener: inspecting must not create, migrate, import, or repair data.
use super::*;
use crate::models::recovery_storage::{WorkspaceStorageEvidence, WorkspaceStorageInspection};
use rusqlite::{limits::Limit, ErrorCode};
use std::io::Read;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Instant;

const DATABASE_BUDGET: u64 = 256 * 1024 * 1024;
const MAX_METADATA_ROWS: usize = 64;
const MAX_METADATA_BYTES: i64 = 65_536;
const QUERY_BUDGET: Duration = Duration::from_millis(500);
const MAX_PROGRESS_CALLS: usize = 2_000;

fn initial(session: Option<String>) -> WorkspaceStorageInspection {
    WorkspaceStorageInspection {
        host_session_id: session,
        sampled_at_unix_ms: crate::services::common::now_unix_ms().to_string(),
        read_only: true,
        barriers_retained: true,
        state: "blocked",
        reason_code: "io-unavailable",
        summary: "本地存储暂不可读；未改写或清理原账本，不授予恢复权限",
        evidence: WorkspaceStorageEvidence {
            database_present: None,
            database_bytes: None,
            application_id: None,
            schema_version: None,
            metadata_rows: None,
            pending_rows: None,
            candidate_bodies_read: false,
            snapshot_stable: false,
        },
    }
}
fn fail(report: &mut WorkspaceStorageInspection, code: &'static str, summary: &'static str) {
    report.state = "blocked";
    report.reason_code = code;
    report.summary = summary;
    report.evidence.metadata_rows = None;
    report.evidence.pending_rows = None;
    report.evidence.snapshot_stable = false;
}
fn io_failure(report: &mut WorkspaceStorageInspection, error: &std::io::Error) {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        fail(
            report,
            "permission-denied",
            "系统拒绝读取本地存储；原账本和未知屏障保留",
        );
    } else {
        fail(
            report,
            "io-unavailable",
            "本地存储读取失败；原账本和未知屏障保留",
        );
    }
}
fn sql_failure(report: &mut WorkspaceStorageInspection, error: &rusqlite::Error) {
    let code = error.sqlite_error_code();
    if matches!(
        code,
        Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
    ) {
        fail(
            report,
            "storage-busy",
            "本地存储正在被其他操作占用；可稍后重新只读检查",
        );
    } else if matches!(
        code,
        Some(ErrorCode::OperationInterrupted | ErrorCode::TooBig)
    ) {
        fail(
            report,
            "inspection-budget",
            "只读检查达到时间、指令或长度预算；未改动原账本",
        );
    } else if matches!(
        code,
        Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase)
    ) {
        fail(
            report,
            "invalid-container",
            "本地数据库容器无法校验；保留原材料，不自动修复或清账",
        );
    } else {
        // Never expose SQLite's message: it may include arbitrary schema text.
        fail(
            report,
            "storage-unavailable",
            "无法完成本地存储只读检查；原材料和屏障保留",
        );
    }
}

fn same_file(before: &fs::Metadata, after: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != after.dev() || before.ino() != after.ino() {
            return false;
        }
    }
    before.len() == after.len() && before.modified().ok() == after.modified().ok()
}

fn companion(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}
fn wal_absent(path: &Path) -> bool {
    fs::symlink_metadata(companion(path, "-wal"))
        .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
}
fn immutable_uri(path: &Path) -> std::io::Result<String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let value = absolute.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "unsupported database path",
        )
    })?;
    // Percent encode every separator, query marker and fragment marker too:
    // no directory name can introduce SQLite URI options or an authority.
    let mut uri = String::from("file:");
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            uri.push(char::from(byte));
        } else {
            use std::fmt::Write;
            let _ = write!(uri, "%{byte:02X}");
        }
    }
    uri.push_str("?immutable=1");
    Ok(uri)
}

pub(crate) fn inspect(dir: &Path, session: Option<String>) -> WorkspaceStorageInspection {
    let mut report = initial(session);
    let path = database_path(dir);
    let before = match fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => meta,
        Ok(_) => {
            report.evidence.database_present = Some(true);
            fail(
                &mut report,
                "unsafe-file",
                "存储位置不是普通文件；未跟随链接或尝试修复",
            );
            return report;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            report.evidence.database_present = Some(false);
            report.state = "not-initialized";
            report.reason_code = "not-initialized";
            report.summary = "尚无本地数据库；检查未创建文件，也不据此解除历史操作屏障";
            return report;
        }
        Err(error) => {
            io_failure(&mut report, &error);
            return report;
        }
    };
    report.evidence.database_present = Some(true);
    report.evidence.database_bytes = Some(before.len().to_string());
    if before.len() > DATABASE_BUDGET {
        fail(
            &mut report,
            "inspection-budget",
            "数据库超过 256 MiB 只读检查预算；原材料保留",
        );
        return report;
    }
    let mut total_bytes = before.len();
    for suffix in ["-wal", "-shm"] {
        match fs::symlink_metadata(companion(&path, suffix)) {
            Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => {
                total_bytes = total_bytes.saturating_add(meta.len());
                if total_bytes > DATABASE_BUDGET {
                    fail(
                        &mut report,
                        "inspection-budget",
                        "数据库与协作文件超过 256 MiB 检查预算；未修改原材料",
                    );
                    return report;
                }
            }
            Ok(_) => {
                fail(
                    &mut report,
                    "unsafe-file",
                    "数据库协作位置不是普通文件；未跟随链接或修复",
                );
                return report;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                io_failure(&mut report, &error);
                return report;
            }
        }
    }
    // SQLite READ_ONLY may still create/modify SHM. Never open the original
    // live WAL namespace, and never silently ignore a committed WAL snapshot.
    if !wal_absent(&path) {
        fail(
            &mut report,
            "wal-active",
            "存在 WAL 协作材料；为保留原始文件，暂不读取主库，请稍后重新检查",
        );
        return report;
    }
    // Read only the fixed SQLite header; candidate and source bodies are not read.
    let mut header = [0u8; 16];
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    match options
        .open(&path)
        .and_then(|mut file| file.read_exact(&mut header))
    {
        Ok(()) if &header == b"SQLite format 3\0" => {}
        Ok(()) => {
            fail(
                &mut report,
                "invalid-container",
                "文件不是可识别的 SQLite 容器；未读取正文或改写原文件",
            );
            return report;
        }
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
            fail(
                &mut report,
                "invalid-container",
                "数据库头部不完整；未改写或替换原文件",
            );
            return report;
        }
        Err(error) => {
            io_failure(&mut report, &error);
            return report;
        }
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_NOFOLLOW
        | OpenFlags::SQLITE_OPEN_URI;
    let uri = match immutable_uri(&path) {
        Ok(uri) => uri,
        Err(error) => {
            io_failure(&mut report, &error);
            return report;
        }
    };
    let connection = match Connection::open_with_flags(uri, flags) {
        Ok(connection) => connection,
        Err(error) => {
            sql_failure(&mut report, &error);
            return report;
        }
    };
    if let Err(error) = configure_readonly(&connection) {
        sql_failure(&mut report, &error);
        return report;
    }
    match inspect_rows(&connection, &mut report) {
        Ok(()) => {}
        Err(error) => sql_failure(&mut report, &error),
    }
    let after = fs::symlink_metadata(&path);
    if !wal_absent(&path)
        || !after.as_ref().is_ok_and(|meta| {
            meta.is_file() && !meta.file_type().is_symlink() && same_file(&before, meta)
        })
    {
        fail(
            &mut report,
            "changed-during-inspection",
            "检查期间存储文件发生变化；结果仅供历史参考，请重新检查",
        );
    }
    report
}

fn configure_readonly(connection: &Connection) -> rusqlite::Result<()> {
    connection.busy_timeout(Duration::from_millis(200))?;
    for (config, enabled) in [
        (DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true),
        (DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false),
        (DbConfig::SQLITE_DBCONFIG_WRITABLE_SCHEMA, false),
        (DbConfig::SQLITE_DBCONFIG_DQS_DDL, false),
        (DbConfig::SQLITE_DBCONFIG_DQS_DML, false),
    ] {
        connection.set_db_config(config, enabled)?;
    }
    connection.pragma_update(None, "query_only", "ON")?;
    connection.pragma_update(None, "temp_store", "MEMORY")?;
    // SQLite rows may contain existing large candidates. Limit returned values
    // without fetching candidate_json; budget trips are never repaired away.
    connection.set_limit(Limit::SQLITE_LIMIT_LENGTH, 1024 * 1024)?;
    connection.set_limit(Limit::SQLITE_LIMIT_SQL_LENGTH, 65_536)?;
    let start = Instant::now();
    let calls = Arc::new(AtomicUsize::new(0));
    connection.progress_handler(
        1_000,
        Some(move || {
            calls.fetch_add(1, Ordering::Relaxed) >= MAX_PROGRESS_CALLS
                || start.elapsed() > QUERY_BUDGET
        }),
    )?;
    Ok(())
}

fn inspect_rows(
    connection: &Connection,
    report: &mut WorkspaceStorageInspection,
) -> rusqlite::Result<()> {
    let transaction = connection.unchecked_transaction()?;
    let application: i32 =
        transaction.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let version: i32 = transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
    report.evidence.application_id = Some(application);
    report.evidence.schema_version = Some(version);
    if application != APPLICATION_ID {
        fail(
            report,
            "wrong-application",
            "数据库应用标识不匹配；未迁移、导入或修改原文件",
        );
        return Ok(());
    }
    if !(4..=SCHEMA_VERSION).contains(&version) {
        fail(
            report,
            "unsupported-schema",
            "此存储版本不支持当前工作区只读检查；未自动迁移或降级",
        );
        return Ok(());
    }
    // Reject a forged view/generated expression before selecting any metadata.
    let regular: bool = transaction.query_row(
        "SELECT count(*)=1 FROM sqlite_schema WHERE name='workspace_recovery' AND type='table' AND sql NOT LIKE 'CREATE VIRTUAL%'",
        [], |row| row.get(0),
    )?;
    let columns: Vec<(String, i64)> = transaction
        .prepare("SELECT name,hidden FROM pragma_table_xinfo('workspace_recovery') LIMIT 8")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    if !regular
        || columns
            != [
                "operation_id",
                "profile_id",
                "stage",
                "version",
                "metadata_json",
                "candidate_json",
            ]
            .map(|name| (name.into(), 0))
    {
        fail(
            report,
            "untrusted-structure",
            "恢复表结构不匹配；未读取候选正文或尝试修复",
        );
        return Ok(());
    }
    let mut statement = transaction.prepare(
        "SELECT operation_id,profile_id,stage,version,CASE WHEN length(CAST(metadata_json AS BLOB))<=65536 THEN metadata_json END,length(CAST(metadata_json AS BLOB)) FROM workspace_recovery ORDER BY operation_id LIMIT 65"
    )?;
    let mut rows = statement.query([])?;
    let mut count = 0;
    let mut pending = 0;
    while let Some(row) = rows.next()? {
        count += 1;
        if count > MAX_METADATA_ROWS {
            fail(
                report,
                "inspection-budget",
                "恢复元资料超过 64 项检查预算；未改动原账本",
            );
            return Ok(());
        }
        let bytes: i64 = row.get(5)?;
        if !(1..=MAX_METADATA_BYTES).contains(&bytes) {
            fail(
                report,
                "invalid-metadata",
                "恢复元资料长度或结构不可信；未读取候选正文，原屏障保留",
            );
            return Ok(());
        }
        let metadata = workspace_recovery::metadata((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
        ));
        let Ok(metadata) = metadata else {
            fail(
                report,
                "invalid-metadata",
                "恢复元资料或冗余身份字段不一致；原记录和屏障保留",
            );
            return Ok(());
        };
        if !matches!(
            metadata.stage,
            workspace_recovery::Stage::Complete | workspace_recovery::Stage::Rejected
        ) {
            pending += 1;
        }
    }
    report.state = "metadata-readable";
    report.reason_code = "metadata-readable";
    report.summary = "恢复元资料可读；候选正文和运行结果未在此校验，此检查不授予恢复或重放权限";
    report.evidence.metadata_rows = Some(count);
    report.evidence.pending_rows = Some(pending);
    report.evidence.snapshot_stable = true;
    Ok(())
}

#[cfg(test)]
mod tests;
