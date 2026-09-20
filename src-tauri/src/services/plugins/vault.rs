//! Host-owned encrypted persistent secrets for signed plugins.
//!
//! Values are namespaced by verified publisher and plugin identity. They never
//! enter plugin configuration, ordinary state exports, diagnostics, or logs.

use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use ring::aead;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

const MASTER_KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const MAX_KEYS: usize = 32;
const MAX_VALUE_BYTES: usize = 16 * 1024;
const MAX_TOTAL_BYTES: usize = 128 * 1024;

#[derive(Default, Serialize, Deserialize)]
#[serde(transparent)]
struct Entries(BTreeMap<String, String>);

fn safe_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.split('/').count() <= 4
        && value.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
}

fn master_key_path(root: &Path) -> PathBuf {
    root.join("vault.key")
}

fn set_private_permissions(path: &Path) -> AppResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(io::failure)?;
    }
    Ok(())
}

fn master_key(root: &Path) -> AppResult<Zeroizing<Vec<u8>>> {
    fs::create_dir_all(root).map_err(io::failure)?;
    let path = master_key_path(root);
    match fs::read(&path) {
        Ok(bytes) if bytes.len() == MASTER_KEY_BYTES => return Ok(Zeroizing::new(bytes)),
        Ok(_) => return Err(AppError::invalid_argument("插件凭据主密钥格式无效")),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(io::failure(error));
        }
        Err(_) => {}
    }
    let mut bytes = vec![0u8; MASTER_KEY_BYTES];
    getrandom::fill(&mut bytes).map_err(io::failure)?;
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            file.write_all(&bytes).map_err(io::failure)?;
            file.sync_all().map_err(io::failure)?;
            set_private_permissions(&path)?;
            Ok(Zeroizing::new(bytes))
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = fs::read(&path).map_err(io::failure)?;
            if existing.len() != MASTER_KEY_BYTES {
                return Err(AppError::invalid_argument("插件凭据主密钥格式无效"));
            }
            Ok(Zeroizing::new(existing))
        }
        Err(error) => Err(io::failure(error)),
    }
}

fn path(root: &Path, publisher: &str, plugin_id: &str) -> AppResult<PathBuf> {
    Ok(namespace::directory(root, publisher, plugin_id)?.join("vault.json"))
}

fn load(root: &Path, publisher: &str, plugin_id: &str) -> AppResult<Entries> {
    let path = path(root, publisher, plugin_id)?;
    match fs::read(&path) {
        Ok(bytes) if bytes.len() <= MAX_TOTAL_BYTES * 2 => {
            serde_json::from_slice(&bytes).map_err(io::failure)
        }
        Ok(_) => Err(AppError::invalid_argument("插件凭据库超过容量限制")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Entries::default()),
        Err(error) => Err(io::failure(error)),
    }
}

fn save(root: &Path, publisher: &str, plugin_id: &str, entries: &Entries) -> AppResult<()> {
    if entries.0.len() > MAX_KEYS {
        return Err(AppError::invalid_argument("插件凭据项超过容量限制"));
    }
    let bytes = serde_json::to_vec(entries).map_err(io::failure)?;
    if bytes.len() > MAX_TOTAL_BYTES * 2 {
        return Err(AppError::invalid_argument("插件凭据库超过容量限制"));
    }
    let target = path(root, publisher, plugin_id)?;
    crate::services::atomic_file::write(&target, &bytes).map_err(io::failure)?;
    set_private_permissions(&target)
}

fn aad(publisher: &str, plugin_id: &str, key: &str) -> Vec<u8> {
    format!("znet-sink/plugin-vault/v1\0{publisher}\0{plugin_id}\0{key}").into_bytes()
}

fn seal(master: &[u8], aad: &[u8], plaintext: &[u8]) -> AppResult<String> {
    let unbound = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, master)
        .map_err(|_| AppError::internal("插件凭据主密钥无效"))?;
    let key = aead::LessSafeKey::new(unbound);
    let mut nonce = [0u8; NONCE_BYTES];
    getrandom::fill(&mut nonce).map_err(io::failure)?;
    let mut value = plaintext.to_vec();
    key.seal_in_place_append_tag(
        aead::Nonce::assume_unique_for_key(nonce),
        aead::Aad::from(aad),
        &mut value,
    )
    .map_err(|_| AppError::internal("无法加密插件凭据"))?;
    let mut encoded = nonce.to_vec();
    encoded.extend_from_slice(&value);
    Ok(STANDARD.encode(encoded))
}

fn open(master: &[u8], aad: &[u8], encoded: &str) -> AppResult<Zeroizing<Vec<u8>>> {
    let mut value = STANDARD
        .decode(encoded)
        .map_err(|_| AppError::invalid_argument("插件凭据密文无效"))?;
    if value.len() < NONCE_BYTES + aead::CHACHA20_POLY1305.tag_len() {
        return Err(AppError::invalid_argument("插件凭据密文无效"));
    }
    let nonce: [u8; NONCE_BYTES] = value[..NONCE_BYTES].try_into().unwrap();
    let mut ciphertext = value.split_off(NONCE_BYTES);
    let unbound = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, master)
        .map_err(|_| AppError::internal("插件凭据主密钥无效"))?;
    let plaintext = aead::LessSafeKey::new(unbound)
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(aad),
            &mut ciphertext,
        )
        .map_err(|_| AppError::invalid_argument("插件凭据校验失败"))?;
    Ok(Zeroizing::new(plaintext.to_vec()))
}

impl Host {
    pub(super) fn vault_get(
        &self,
        plugin_id: &str,
        key: &str,
    ) -> AppResult<Option<Zeroizing<Vec<u8>>>> {
        if !safe_key(key) {
            return Err(AppError::invalid_argument("插件凭据键无效"));
        }
        let publisher = self.namespace_publisher(plugin_id)?;
        let root = self.root()?;
        let entries = load(&root, &publisher, plugin_id)?;
        let Some(value) = entries.0.get(key) else {
            return Ok(None);
        };
        open(&master_key(&root)?, &aad(&publisher, plugin_id, key), value).map(Some)
    }

    pub(super) fn vault_put(&self, plugin_id: &str, key: &str, value: &[u8]) -> AppResult<()> {
        if !safe_key(key) || value.is_empty() || value.len() > MAX_VALUE_BYTES {
            return Err(AppError::invalid_argument("插件凭据键或内容无效"));
        }
        let publisher = self.namespace_publisher(plugin_id)?;
        let root = self.root()?;
        let mut entries = load(&root, &publisher, plugin_id)?;
        entries.0.insert(
            key.to_owned(),
            seal(&master_key(&root)?, &aad(&publisher, plugin_id, key), value)?,
        );
        save(&root, &publisher, plugin_id, &entries)
    }

    pub(super) fn vault_delete(&self, plugin_id: &str, key: &str) -> AppResult<bool> {
        if !safe_key(key) {
            return Err(AppError::invalid_argument("插件凭据键无效"));
        }
        let publisher = self.namespace_publisher(plugin_id)?;
        let root = self.root()?;
        let mut entries = load(&root, &publisher, plugin_id)?;
        let removed = entries.0.remove(key).is_some();
        save(&root, &publisher, plugin_id, &entries)?;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_value_is_bound_to_identity_and_key_name() {
        let master = [7u8; MASTER_KEY_BYTES];
        let first = aad("publisher", "plugin", "session/access");
        let encoded = seal(&master, &first, b"credential").unwrap();
        assert!(!encoded.contains("credential"));
        assert_eq!(
            open(&master, &first, &encoded).unwrap().as_slice(),
            b"credential"
        );
        assert!(open(
            &master,
            &aad("publisher", "plugin", "session/renewal"),
            &encoded
        )
        .is_err());
    }
}
