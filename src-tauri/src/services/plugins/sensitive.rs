//! In-memory opaque material handles. Values are never serialized, logged, or
//! returned by handle lookup. They are scoped to one signed component and expire.

use super::*;
use std::{collections::BTreeMap, sync::Mutex};
use zeroize::Zeroizing;

const MAX_HANDLES: usize = 64;
const MAX_VALUE_BYTES: usize = 8 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
const MAX_LIFETIME_MS: u64 = 10 * 60 * 1000;

#[derive(Default)]
pub(super) struct Store {
    entries: Mutex<BTreeMap<String, Entry>>,
}

struct Entry {
    owner: String,
    purpose: String,
    expires_at_unix_ms: u64,
    bytes: Zeroizing<Vec<u8>>,
}

impl Store {
    pub(super) fn insert(
        &self,
        owner: String,
        purpose: String,
        bytes: Vec<u8>,
        lifetime_ms: u64,
    ) -> AppResult<(String, u64)> {
        if bytes.is_empty()
            || bytes.len() > MAX_VALUE_BYTES
            || purpose.is_empty()
            || purpose.len() > 64
            || lifetime_ms == 0
            || lifetime_ms > MAX_LIFETIME_MS
        {
            return Err(AppError::invalid_argument("敏感材料大小、用途或有效期无效"));
        }
        let now = crate::services::common::now_unix_ms();
        let mut entries = self.entries.lock().unwrap();
        entries.retain(|_, entry| entry.expires_at_unix_ms > now);
        let total: usize = entries.values().map(|entry| entry.bytes.len()).sum();
        if entries.len() >= MAX_HANDLES || total.saturating_add(bytes.len()) > MAX_TOTAL_BYTES {
            return Err(AppError::invalid_argument("敏感材料句柄达到容量限制"));
        }
        let mut random = [0u8; 24];
        getrandom::fill(&mut random).map_err(io::failure)?;
        let handle = format!(
            "s1_{}",
            random
                .iter()
                .map(|value| format!("{value:02x}"))
                .collect::<String>()
        );
        let expires_at_unix_ms = now.saturating_add(lifetime_ms);
        entries.insert(
            handle.clone(),
            Entry {
                owner,
                purpose,
                expires_at_unix_ms,
                bytes: Zeroizing::new(bytes),
            },
        );
        Ok((handle, expires_at_unix_ms))
    }

    pub(super) fn clone_bytes(
        &self,
        owner: &str,
        handle: &str,
        purpose: Option<&str>,
    ) -> AppResult<Zeroizing<Vec<u8>>> {
        let now = crate::services::common::now_unix_ms();
        let mut entries = self.entries.lock().unwrap();
        entries.retain(|_, entry| entry.expires_at_unix_ms > now);
        let entry = entries
            .get(handle)
            .filter(|entry| entry.owner == owner)
            .filter(|entry| purpose.is_none_or(|purpose| entry.purpose == purpose))
            .ok_or_else(|| AppError::not_found("sensitive_material", "expired-or-unavailable"))?;
        Ok(Zeroizing::new(entry.bytes.to_vec()))
    }

    pub(super) fn take(
        &self,
        owner: &str,
        handle: &str,
        purpose: Option<&str>,
    ) -> AppResult<Zeroizing<Vec<u8>>> {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries
            .remove(handle)
            .filter(|entry| entry.owner == owner)
            .filter(|entry| purpose.is_none_or(|purpose| entry.purpose == purpose))
            .filter(|entry| entry.expires_at_unix_ms > crate::services::common::now_unix_ms())
            .ok_or_else(|| AppError::not_found("sensitive_material", "expired-or-unavailable"))?;
        Ok(entry.bytes)
    }

    pub(super) fn remove(&self, owner: &str, handle: &str) -> AppResult<()> {
        let mut entries = self.entries.lock().unwrap();
        if entries
            .get(handle)
            .is_some_and(|entry| entry.owner == owner)
        {
            entries.remove(handle);
            return Ok(());
        }
        Err(AppError::not_found(
            "sensitive_material",
            "expired-or-unavailable",
        ))
    }

    pub(super) fn clear_owner(&self, owner: &str) {
        self.entries
            .lock()
            .unwrap()
            .retain(|_, entry| entry.owner != owner);
    }

    pub(super) fn clear_plugin(&self, plugin_id: &str) {
        let prefix = format!("{plugin_id}/");
        self.entries
            .lock()
            .unwrap()
            .retain(|_, entry| !entry.owner.starts_with(&prefix));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_are_owner_scoped_one_shot_and_memory_only() {
        let store = Store::default();
        let (handle, _) = store
            .insert("plugin/a".into(), "material".into(), vec![1, 2, 3], 1_000)
            .unwrap();
        assert!(store.clone_bytes("plugin/b", &handle, None).is_err());
        assert_eq!(
            store
                .take("plugin/a", &handle, Some("material"))
                .unwrap()
                .as_slice(),
            &[1, 2, 3]
        );
        assert!(store.take("plugin/a", &handle, None).is_err());
    }

    #[test]
    fn lifetime_and_size_are_bounded() {
        let store = Store::default();
        assert!(store
            .insert("plugin/a".into(), "secret".into(), vec![1], 0)
            .is_err());
        assert!(store
            .insert(
                "plugin/a".into(),
                "secret".into(),
                vec![1],
                MAX_LIFETIME_MS + 1
            )
            .is_err());
        assert!(store
            .insert(
                "plugin/a".into(),
                "secret".into(),
                vec![0; MAX_VALUE_BYTES + 1],
                1
            )
            .is_err());
    }
}
