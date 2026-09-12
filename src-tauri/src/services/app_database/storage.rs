//! Ordinary bounded storage. Caller identity is taken from a host-issued lease;
//! neither SQL nor database paths nor another owner's namespace are guest inputs.
use super::*;
use znet_client_core::capability::{Error, Lease, Permission, Resource};

pub(super) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS capability_storage_clock (
    id INTEGER PRIMARY KEY CHECK(id=1),
    revision INTEGER NOT NULL CHECK(revision >= 0)
) STRICT;
INSERT OR IGNORE INTO capability_storage_clock VALUES(1,0);
CREATE TABLE IF NOT EXISTS capability_values (
    owner TEXT NOT NULL CHECK(length(owner) BETWEEN 1 AND 512),
    key TEXT NOT NULL CHECK(length(key) BETWEEN 1 AND 128),
    value BLOB NOT NULL CHECK(length(value) <= 65536),
    revision INTEGER NOT NULL CHECK(revision > 0),
    PRIMARY KEY(owner, key)
) STRICT;
"#;
const OWNER_BYTES: i64 = 1024 * 1024;
const GLOBAL_BYTES: i64 = 64 * 1024 * 1024;
const OWNER_ITEMS: i64 = 256;
const GLOBAL_ITEMS: i64 = 16384;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub revision: i64,
    pub value: Vec<u8>,
}
pub(crate) enum Change {
    /// None means create only; Some is an exact revision, never blind overwrite.
    Put {
        key: String,
        expected: Option<i64>,
        value: Vec<u8>,
    },
    Delete {
        key: String,
        expected: i64,
    },
}
fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 128 && !key.chars().any(char::is_control)
}
fn owner(lease: &Lease) -> Result<&str, Error> {
    let value = lease.identity();
    if value.is_empty() || value.len() > 512 {
        return Err(Error::InvalidRequest);
    }
    Ok(value)
}
fn db<T>(result: rusqlite::Result<T>) -> Result<T, Error> {
    result.map_err(|_| Error::Transport)
}
/// Host chooses the database directory; adapters never accept it from guests.
pub(crate) fn get(dir: &Path, lease: &Lease, key: &str) -> Result<Resource<Option<Entry>>, Error> {
    let permission = Permission::new("storage.read", "self");
    lease.execute(&permission, || {
        if !valid_key(key) {
            return Err(Error::InvalidRequest);
        }
        let connection = open(dir).map_err(|_| Error::Transport)?;
        lease.check(Some(&permission))?;
        let entry = db(connection
            .query_row(
                "SELECT revision, value FROM capability_values WHERE owner=?1 AND key=?2",
                params![owner(lease)?, key],
                |row| {
                    Ok(Entry {
                        revision: row.get(0)?,
                        value: row.get(1)?,
                    })
                },
            )
            .optional())?;
        let bytes = entry.as_ref().map_or(0, |v| v.value.len() + 8);
        Ok((entry, bytes))
    })
}
/// A batch is one SQLite transaction. Quotas and revision checks occur under its
/// write lock, so competing connections cannot both consume the same capacity.
pub(crate) fn change(
    dir: &Path,
    lease: &Lease,
    changes: &[Change],
) -> Result<Resource<Vec<i64>>, Error> {
    let permission = Permission::new("storage.write", "self");
    lease.execute(&permission, || {
        if changes.is_empty() || changes.len() > 32 { return Err(Error::BudgetExceeded); }
        let namespace = owner(lease)?;
        let mut connection = open(dir).map_err(|_| Error::Transport)?;
        let tx = db(connection.transaction_with_behavior(TransactionBehavior::Immediate))?;
        let mut seen = HashSet::new();
        let mut revisions = Vec::new();
        for change in changes {
            lease.check(Some(&permission))?;
            let (key, expected) = match change {
                Change::Put { key, expected, .. } => (key, *expected),
                Change::Delete { key, expected } => (key, Some(*expected)),
            };
            if !valid_key(key) || !seen.insert(key) || expected.is_some_and(|n| n <= 0) {
                return Err(Error::InvalidRequest);
            }
            let current: Option<i64> = db(tx.query_row(
                "SELECT revision FROM capability_values WHERE owner=?1 AND key=?2",
                params![namespace, key], |row| row.get(0),
            ).optional())?;
            if current != expected { return Err(Error::Busy); }
            let previous: i64 = db(tx.query_row("SELECT revision FROM capability_storage_clock WHERE id=1", [], |r| r.get(0)))?;
            let revision = previous.checked_add(1).ok_or(Error::BudgetExceeded)?;
            db(tx.execute("UPDATE capability_storage_clock SET revision=?1 WHERE id=1", [revision]))?;
            revisions.push(revision);
            match change {
                Change::Put { value, .. } => {
                    if value.len() > 65536 { return Err(Error::BudgetExceeded); }
                    db(tx.execute("INSERT INTO capability_values(owner,key,value,revision) VALUES(?1,?2,?3,?4) ON CONFLICT(owner,key) DO UPDATE SET value=excluded.value,revision=excluded.revision", params![namespace, key, value, revision]))?;
                }
                Change::Delete { .. } => {
                    db(tx.execute("DELETE FROM capability_values WHERE owner=?1 AND key=?2", params![namespace, key]))?;
                }
            }
        }
        for (sql, parameter, bytes, count) in [
            ("SELECT coalesce(sum(length(CAST(key AS BLOB))+length(value)),0),count(*) FROM capability_values WHERE owner=?1", namespace, OWNER_BYTES, OWNER_ITEMS),
            ("SELECT coalesce(sum(length(CAST(owner AS BLOB))+length(CAST(key AS BLOB))+length(value)),0),count(*) FROM capability_values WHERE ?1 IS NOT NULL", namespace, GLOBAL_BYTES, GLOBAL_ITEMS),
        ] {
            let (used, items): (i64, i64) = db(tx.query_row(sql, [parameter], |r| Ok((r.get(0)?, r.get(1)?))))?;
            if used > bytes || items > count { return Err(Error::BudgetExceeded); }
        }
        lease.check(Some(&permission))?;
        commit_and_restrict(tx, dir).map_err(|_| Error::Transport)?;
        let bytes = revisions.len() * 8;
        Ok((revisions, bytes))
    })
}

#[cfg(test)]
mod tests;
