use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::config::DbConfig;
use rusqlite::{
    params, Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
};

use crate::errors::{AppError, AppResult};
use crate::models::proxy_config::ProxyConfigProfile;
use crate::models::subscription::SubscriptionProfile;

use super::domain_store;

const DATABASE_FILE: &str = "znet-sink.db";
const SCHEMA_VERSION: i32 = 1;
const APPLICATION_ID: i32 = 0x5A4E_4554; // "ZNET"
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RECORD_JSON_BYTES: usize = 16 * 1024 * 1024;
const MAX_SUBSCRIPTION_URL_BYTES: usize = 8 * 1024;

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS app_metadata (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS proxy_configs (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(id) BETWEEN 1 AND 512),
    position INTEGER NOT NULL CHECK (position >= 0),
    active INTEGER NOT NULL CHECK (active IN (0, 1)),
    payload_json TEXT NOT NULL
        CHECK (json_valid(payload_json) AND length(payload_json) <= 16777216)
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS proxy_configs_single_active
    ON proxy_configs(active) WHERE active = 1;
CREATE INDEX IF NOT EXISTS proxy_configs_position
    ON proxy_configs(position);

CREATE TABLE IF NOT EXISTS subscriptions (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(id) BETWEEN 1 AND 512),
    position INTEGER NOT NULL CHECK (position >= 0),
    source_url TEXT NOT NULL
        CHECK (length(source_url) BETWEEN 1 AND 8192),
    target_proxy_config_id TEXT,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    payload_json TEXT NOT NULL
        CHECK (json_valid(payload_json) AND length(payload_json) <= 16777216),
    FOREIGN KEY (target_proxy_config_id) REFERENCES proxy_configs(id)
        ON UPDATE CASCADE ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE INDEX IF NOT EXISTS subscriptions_position
    ON subscriptions(position);
CREATE INDEX IF NOT EXISTS subscriptions_source_url
    ON subscriptions(source_url);
CREATE INDEX IF NOT EXISTS subscriptions_target_proxy_config
    ON subscriptions(target_proxy_config_id);
"#;

pub(crate) struct RelationalDomainData {
    pub proxy_configs: Vec<ProxyConfigProfile>,
    pub subscriptions: Vec<SubscriptionProfile>,
}

pub(crate) fn load_domain_data(dir: &Path) -> AppResult<RelationalDomainData> {
    let mut connection = open(dir)?;
    import_legacy_json_once(&mut connection, dir)?;
    Ok(RelationalDomainData {
        proxy_configs: load_proxy_configs(&connection)?,
        subscriptions: load_subscriptions(&connection)?,
    })
}

pub(crate) fn save_proxy_configs(dir: &Path, items: &[ProxyConfigProfile]) -> AppResult<()> {
    let mut connection = open(dir)?;
    import_legacy_json_once(&mut connection, dir)?;
    let transaction = immediate_transaction(&mut connection)?;
    replace_proxy_configs(&transaction, items)?;
    commit_and_restrict(transaction, dir)
}

pub(crate) fn save_subscriptions(dir: &Path, items: &[SubscriptionProfile]) -> AppResult<()> {
    let mut connection = open(dir)?;
    import_legacy_json_once(&mut connection, dir)?;
    let transaction = immediate_transaction(&mut connection)?;
    replace_subscriptions(&transaction, items)?;
    commit_and_restrict(transaction, dir)
}

pub(crate) fn save_domain_data(
    dir: &Path,
    proxy_configs: &[ProxyConfigProfile],
    subscriptions: &[SubscriptionProfile],
) -> AppResult<()> {
    let mut connection = open(dir)?;
    import_legacy_json_once(&mut connection, dir)?;
    let transaction = immediate_transaction(&mut connection)?;
    replace_proxy_configs(&transaction, proxy_configs)?;
    replace_subscriptions(&transaction, subscriptions)?;
    commit_and_restrict(transaction, dir)
}

fn open(dir: &Path) -> AppResult<Connection> {
    fs::create_dir_all(dir).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to create application data directory: {error}"),
        details: Some(serde_json::json!({ "path": dir.display().to_string() })),
    })?;
    let path = database_path(dir);
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
        | OpenFlags::SQLITE_OPEN_CREATE
        | OpenFlags::SQLITE_OPEN_FULL_MUTEX;
    let mut connection =
        Connection::open_with_flags(&path, flags).map_err(|error| database_error(dir, error))?;
    configure_connection(&connection, dir)?;
    migrate_schema(&mut connection, dir)?;
    verify_integrity(&connection, dir)?;
    restrict_database_permissions(&path)?;
    Ok(connection)
}

fn configure_connection(connection: &Connection, dir: &Path) -> AppResult<()> {
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(|error| database_error(dir, error))?;
    for (config, enabled) in [
        (DbConfig::SQLITE_DBCONFIG_ENABLE_FKEY, true),
        (DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true),
        (DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false),
        (DbConfig::SQLITE_DBCONFIG_WRITABLE_SCHEMA, false),
        (DbConfig::SQLITE_DBCONFIG_DQS_DDL, false),
        (DbConfig::SQLITE_DBCONFIG_DQS_DML, false),
    ] {
        connection
            .set_db_config(config, enabled)
            .map_err(|error| database_error(dir, error))?;
    }
    let journal_mode: String = connection
        .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
        .map_err(|error| database_error(dir, error))?;
    if !journal_mode.eq_ignore_ascii_case("wal") {
        return Err(AppError::internal("failed to enable SQLite WAL mode"));
    }
    for (name, value) in [
        ("synchronous", "FULL"),
        ("secure_delete", "ON"),
        ("temp_store", "MEMORY"),
        ("wal_autocheckpoint", "1000"),
        ("journal_size_limit", "16777216"),
    ] {
        connection
            .pragma_update(None, name, value)
            .map_err(|error| database_error(dir, error))?;
    }
    Ok(())
}

fn migrate_schema(connection: &mut Connection, dir: &Path) -> AppResult<()> {
    let application_id: i32 = connection
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(|error| database_error(dir, error))?;
    if application_id != 0 && application_id != APPLICATION_ID {
        return Err(AppError::invalid_argument(
            "application database belongs to a different application",
        ));
    }
    let version: i32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| database_error(dir, error))?;
    if version > SCHEMA_VERSION {
        return Err(AppError::invalid_argument(format!(
            "application database schema {version} is newer than supported schema {SCHEMA_VERSION}"
        )));
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }

    let transaction = immediate_transaction(connection)?;
    if version == 0 {
        transaction
            .execute_batch(SCHEMA_V1)
            .map_err(|error| database_error(dir, error))?;
        transaction
            .pragma_update(None, "application_id", APPLICATION_ID)
            .map_err(|error| database_error(dir, error))?;
        transaction
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(|error| database_error(dir, error))?;
    }
    commit_and_restrict(transaction, dir)
}

fn import_legacy_json_once(connection: &mut Connection, dir: &Path) -> AppResult<()> {
    let imported = connection
        .query_row(
            "SELECT value FROM app_metadata WHERE key = 'legacy_json_imported'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| database_error(dir, error))?
        .is_some();
    if imported {
        return Ok(());
    }

    let mut proxy_configs: Vec<ProxyConfigProfile> =
        domain_store::load_legacy_vec(&domain_store::legacy_proxy_configs_path(dir))?;
    normalize_active_profile(&mut proxy_configs);
    let proxy_ids = proxy_configs
        .iter()
        .map(|profile| profile.id.as_str())
        .collect::<HashSet<_>>();
    let mut subscriptions: Vec<SubscriptionProfile> =
        domain_store::load_legacy_vec(&domain_store::legacy_subscriptions_path(dir))?;
    for subscription in &mut subscriptions {
        if subscription
            .target_proxy_config_id
            .as_deref()
            .is_some_and(|id| !proxy_ids.contains(id))
        {
            subscription.target_proxy_config_id = None;
        }
    }

    let transaction = immediate_transaction(connection)?;
    let row_count: i64 = transaction
        .query_row(
            "SELECT (SELECT count(*) FROM proxy_configs) + (SELECT count(*) FROM subscriptions)",
            [],
            |row| row.get(0),
        )
        .map_err(|error| database_error(dir, error))?;
    if row_count == 0 {
        replace_proxy_configs(&transaction, &proxy_configs)?;
        replace_subscriptions(&transaction, &subscriptions)?;
    }
    transaction
        .execute(
            "INSERT INTO app_metadata(key, value) VALUES('legacy_json_imported', '1')",
            [],
        )
        .map_err(|error| database_error(dir, error))?;
    commit_and_restrict(transaction, dir)
}

fn load_proxy_configs(connection: &Connection) -> AppResult<Vec<ProxyConfigProfile>> {
    let mut statement = connection
        .prepare("SELECT payload_json FROM proxy_configs ORDER BY position")
        .map_err(|error| database_error_from_connection(connection, error))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| database_error_from_connection(connection, error))?;
    let mut result = Vec::new();
    for row in rows {
        let payload = row.map_err(|error| database_error_from_connection(connection, error))?;
        result.push(serde_json::from_str(&payload).map_err(|error| {
            AppError::internal(format!(
                "failed to decode proxy_configs database record: {error}"
            ))
        })?);
    }
    Ok(result)
}

fn load_subscriptions(connection: &Connection) -> AppResult<Vec<SubscriptionProfile>> {
    let mut statement = connection
        .prepare(
            "SELECT payload_json, source_url, target_proxy_config_id \
             FROM subscriptions ORDER BY position",
        )
        .map_err(|error| database_error_from_connection(connection, error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|error| database_error_from_connection(connection, error))?;
    let mut result = Vec::new();
    for row in rows {
        let (payload, source_url, target_proxy_config_id) =
            row.map_err(|error| database_error_from_connection(connection, error))?;
        let mut profile: SubscriptionProfile = serde_json::from_str(&payload).map_err(|error| {
            AppError::internal(format!(
                "failed to decode subscriptions database record: {error}"
            ))
        })?;
        profile.url = source_url;
        profile.target_proxy_config_id = target_proxy_config_id;
        result.push(profile);
    }
    Ok(result)
}

fn replace_proxy_configs(
    transaction: &Transaction<'_>,
    items: &[ProxyConfigProfile],
) -> AppResult<()> {
    transaction
        .execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS incoming_proxy_configs(id TEXT PRIMARY KEY) WITHOUT ROWID;\
             DELETE FROM incoming_proxy_configs;\
             UPDATE proxy_configs SET active = 0 WHERE active = 1;",
        )
        .map_err(database_error_without_path)?;
    let mut insert = transaction
        .prepare_cached(
            "INSERT INTO proxy_configs(id, position, active, payload_json) VALUES(?1, ?2, ?3, ?4) \
             ON CONFLICT(id) DO UPDATE SET \
                 position=excluded.position, active=excluded.active, payload_json=excluded.payload_json",
        )
        .map_err(database_error_without_path)?;
    let mut mark = transaction
        .prepare_cached("INSERT INTO incoming_proxy_configs(id) VALUES(?1)")
        .map_err(database_error_without_path)?;
    for (position, profile) in items.iter().enumerate() {
        let payload = serialize_record("proxy config", profile)?;
        insert
            .execute(params![
                profile.id,
                position as i64,
                profile.active,
                payload
            ])
            .map_err(database_error_without_path)?;
        mark.execute(params![profile.id])
            .map_err(database_error_without_path)?;
    }
    drop(insert);
    drop(mark);
    transaction
        .execute(
            "DELETE FROM proxy_configs \
             WHERE NOT EXISTS (SELECT 1 FROM incoming_proxy_configs WHERE id = proxy_configs.id)",
            [],
        )
        .map_err(database_error_without_path)?;
    Ok(())
}

fn replace_subscriptions(
    transaction: &Transaction<'_>,
    items: &[SubscriptionProfile],
) -> AppResult<()> {
    transaction
        .execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS incoming_subscriptions(id TEXT PRIMARY KEY) WITHOUT ROWID;\
             DELETE FROM incoming_subscriptions;",
        )
        .map_err(database_error_without_path)?;
    let mut insert = transaction
        .prepare_cached(
            "INSERT INTO subscriptions(\
                 id, position, source_url, target_proxy_config_id, enabled, payload_json\
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(id) DO UPDATE SET \
                 position=excluded.position, source_url=excluded.source_url, \
                 target_proxy_config_id=excluded.target_proxy_config_id, \
                 enabled=excluded.enabled, payload_json=excluded.payload_json",
        )
        .map_err(database_error_without_path)?;
    let mut mark = transaction
        .prepare_cached("INSERT INTO incoming_subscriptions(id) VALUES(?1)")
        .map_err(database_error_without_path)?;
    for (position, profile) in items.iter().enumerate() {
        if profile.url.len() > MAX_SUBSCRIPTION_URL_BYTES {
            return Err(AppError::invalid_argument(
                "subscription URL exceeds the storage limit",
            ));
        }
        let payload = serialize_record("subscription", profile)?;
        insert
            .execute(params![
                profile.id,
                position as i64,
                profile.url,
                profile.target_proxy_config_id,
                profile.enabled,
                payload
            ])
            .map_err(database_error_without_path)?;
        mark.execute(params![profile.id])
            .map_err(database_error_without_path)?;
    }
    drop(insert);
    drop(mark);
    transaction
        .execute(
            "DELETE FROM subscriptions \
             WHERE NOT EXISTS (SELECT 1 FROM incoming_subscriptions WHERE id = subscriptions.id)",
            [],
        )
        .map_err(database_error_without_path)?;
    Ok(())
}

fn serialize_record<T: serde::Serialize>(kind: &str, value: &T) -> AppResult<String> {
    let payload = serde_json::to_string(value).map_err(|error| {
        AppError::internal(format!(
            "failed to serialize {kind} database record: {error}"
        ))
    })?;
    if payload.len() > MAX_RECORD_JSON_BYTES {
        return Err(AppError::invalid_argument(format!(
            "{kind} exceeds the storage limit"
        )));
    }
    Ok(payload)
}

fn normalize_active_profile(items: &mut [ProxyConfigProfile]) {
    let mut active_seen = false;
    for item in items.iter_mut() {
        if item.active && !active_seen {
            active_seen = true;
        } else {
            item.active = false;
        }
    }
    if !active_seen {
        if let Some(first) = items.first_mut() {
            first.active = true;
        }
    }
}

fn verify_integrity(connection: &Connection, dir: &Path) -> AppResult<()> {
    let result: String = connection
        .pragma_query_value(None, "quick_check", |row| row.get(0))
        .map_err(|error| database_error(dir, error))?;
    if result != "ok" {
        return Err(AppError::internal(
            "application database integrity check failed",
        ));
    }
    Ok(())
}

fn immediate_transaction(connection: &mut Connection) -> AppResult<Transaction<'_>> {
    connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(database_error_without_path)
}

fn commit_and_restrict(transaction: Transaction<'_>, dir: &Path) -> AppResult<()> {
    transaction
        .commit()
        .map_err(|error| database_error(dir, error))?;
    restrict_database_permissions(&database_path(dir))
}

fn database_path(dir: &Path) -> PathBuf {
    dir.join(DATABASE_FILE)
}

fn database_error(dir: &Path, error: rusqlite::Error) -> AppError {
    AppError {
        code: "io_error",
        message: format!("application database operation failed: {error}"),
        details: Some(serde_json::json!({ "path": database_path(dir).display().to_string() })),
    }
}

fn database_error_from_connection(connection: &Connection, error: rusqlite::Error) -> AppError {
    let path = connection
        .path()
        .map(str::to_string)
        .unwrap_or_else(|| DATABASE_FILE.to_string());
    AppError {
        code: "io_error",
        message: format!("application database operation failed: {error}"),
        details: Some(serde_json::json!({ "path": path })),
    }
}

fn database_error_without_path(error: rusqlite::Error) -> AppError {
    AppError {
        code: "io_error",
        message: format!("application database operation failed: {error}"),
        details: None,
    }
}

#[cfg(unix)]
fn restrict_database_permissions(path: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;

    for candidate in [
        path.to_path_buf(),
        PathBuf::from(format!("{}-wal", path.display())),
        PathBuf::from(format!("{}-shm", path.display())),
    ] {
        if candidate.exists() {
            fs::set_permissions(&candidate, fs::Permissions::from_mode(0o600)).map_err(
                |error| AppError {
                    code: "io_error",
                    message: format!("failed to restrict database permissions: {error}"),
                    details: Some(serde_json::json!({ "path": candidate.display().to_string() })),
                },
            )?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn restrict_database_permissions(_path: &Path) -> AppResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("znet-sink-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn database_connections_enable_hardening_controls() {
        let dir = test_dir("sqlite-hardening");
        let connection = open(&dir).unwrap();

        let journal_mode: String = connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        let synchronous: i32 = connection
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .unwrap();
        let secure_delete: i32 = connection
            .pragma_query_value(None, "secure_delete", |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode, "wal");
        assert_eq!(synchronous, 2);
        assert_eq!(secure_delete, 1);
        assert!(connection
            .db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_FKEY)
            .unwrap());
        assert!(connection
            .db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE)
            .unwrap());
        assert!(!connection
            .db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA)
            .unwrap());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let database = database_path(&dir);
            for candidate in [
                database.clone(),
                PathBuf::from(format!("{}-wal", database.display())),
                PathBuf::from(format!("{}-shm", database.display())),
            ] {
                if candidate.exists() {
                    assert_eq!(
                        fs::metadata(candidate).unwrap().permissions().mode() & 0o777,
                        0o600
                    );
                }
            }
        }

        drop(connection);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_json_import_is_idempotent_and_repairs_orphan_targets() {
        let dir = test_dir("sqlite-legacy-import");
        fs::create_dir_all(&dir).unwrap();
        let proxy = proxy_profile("proxy-1");
        let orphan = subscription_profile(
            "subscription-1",
            "https://example.com/original",
            Some("missing-proxy".to_string()),
        );
        fs::write(
            domain_store::legacy_proxy_configs_path(&dir),
            serde_json::to_vec(&vec![proxy]).unwrap(),
        )
        .unwrap();
        fs::write(
            domain_store::legacy_subscriptions_path(&dir),
            serde_json::to_vec(&vec![orphan]).unwrap(),
        )
        .unwrap();

        let imported = load_domain_data(&dir).unwrap();
        assert_eq!(imported.proxy_configs.len(), 1);
        assert_eq!(imported.subscriptions.len(), 1);
        assert!(imported.subscriptions[0].target_proxy_config_id.is_none());

        let changed = subscription_profile(
            "subscription-1",
            "https://example.com/changed",
            Some("proxy-1".to_string()),
        );
        fs::write(
            domain_store::legacy_subscriptions_path(&dir),
            serde_json::to_vec(&vec![changed]).unwrap(),
        )
        .unwrap();
        let reloaded = load_domain_data(&dir).unwrap();
        assert_eq!(
            reloaded.subscriptions[0].url,
            "https://example.com/original"
        );
        assert!(reloaded.subscriptions[0].target_proxy_config_id.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_relationship_does_not_partially_replace_subscriptions() {
        let dir = test_dir("sqlite-transaction-rollback");
        let proxy = proxy_profile("proxy-1");
        save_proxy_configs(&dir, &[proxy]).unwrap();
        let valid = subscription_profile(
            "subscription-1",
            "https://example.com/valid",
            Some("proxy-1".to_string()),
        );
        save_subscriptions(&dir, &[valid]).unwrap();

        let invalid = subscription_profile(
            "subscription-1",
            "https://example.com/not-committed",
            Some("missing-proxy".to_string()),
        );
        assert!(save_subscriptions(&dir, &[invalid]).is_err());
        let reloaded = load_domain_data(&dir).unwrap();
        assert_eq!(reloaded.subscriptions[0].url, "https://example.com/valid");
        let _ = fs::remove_dir_all(&dir);
    }

    fn proxy_profile(id: &str) -> ProxyConfigProfile {
        ProxyConfigProfile {
            id: id.to_string(),
            name: "Local".to_string(),
            kernel: "zero".to_string(),
            format: "json".to_string(),
            path: None,
            content: Some(serde_json::json!({ "outbounds": [] })),
            active: true,
            updated_at_unix_ms: 1,
            capabilities: Default::default(),
        }
    }

    fn subscription_profile(
        id: &str,
        url: &str,
        target_proxy_config_id: Option<String>,
    ) -> SubscriptionProfile {
        SubscriptionProfile {
            id: id.to_string(),
            name: "Remote".to_string(),
            url: url.to_string(),
            enabled: true,
            kernel: "zero".to_string(),
            format: "auto".to_string(),
            target_proxy_config_id,
            policy_selections: Default::default(),
            update_interval_secs: None,
            user_agent: None,
            node_count: None,
            upload_bytes: None,
            download_bytes: None,
            total_bytes: None,
            expire_at_unix_ms: None,
            updated_at_unix_ms: 1,
            last_sync_at_unix_ms: None,
            last_error: None,
        }
    }
}
