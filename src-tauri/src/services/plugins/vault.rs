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
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

const MASTER_KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const MAX_KEYS: usize = 32;
const MAX_VALUE_BYTES: usize = 16 * 1024;
const MAX_TOTAL_BYTES: usize = 128 * 1024;
pub(super) type KeyReader =
    Arc<dyn Fn(bool) -> AppResult<Option<Zeroizing<Vec<u8>>>> + Send + Sync>;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyIdentity {
    schema_version: u32,
    key_id: String,
}
pub(super) fn unavailable() -> AppError {
    AppError {
        code: "plugin_secure_store_unavailable",
        message: "平台安全存储未确认或密钥不可用；不会改用普通文件或替换既有密钥".into(),
        details: None,
    }
}
fn read_identity(path: &Path) -> AppResult<KeyIdentity> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path).map_err(|_| unavailable())?;
    let metadata = file.metadata().map_err(|_| unavailable())?;
    if !metadata.is_file() || metadata.len() > 256 {
        return Err(unavailable());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(unavailable());
        }
    }
    let mut bytes = Vec::new();
    file.take(257)
        .read_to_end(&mut bytes)
        .map_err(|_| unavailable())?;
    if bytes.len() > 256 {
        return Err(unavailable());
    }
    serde_json::from_slice(&bytes).map_err(|_| unavailable())
}
fn platform_key(
    root: &Path,
    provider: &KeyReader,
    create: bool,
    check: &dyn Fn() -> AppResult<()>,
) -> AppResult<Zeroizing<Vec<u8>>> {
    check()?;
    // Mobile has never supported this plaintext master format. Migration needs
    // its own reviewed path; silently preferring it would defeat platform custody.
    match fs::symlink_metadata(master_key_path(root)) {
        Ok(_) => return Err(unavailable()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(_) => return Err(unavailable()),
    }
    let marker = root.join("vault.platform-identity.v1.json");
    let identity: Option<KeyIdentity> = match fs::symlink_metadata(&marker) {
        Ok(metadata) if metadata.is_file() && metadata.len() <= 256 => {
            Some(read_identity(&marker)?)
        }
        Ok(_) => return Err(unavailable()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(unavailable()),
    };
    if identity.as_ref().is_some_and(|value| {
        value.schema_version != 1
            || value.key_id.len() != 64
            || !value
                .key_id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    }) {
        return Err(unavailable());
    }
    if identity.is_none() && !create {
        return Err(unavailable());
    }
    let key = provider(create && identity.is_none())?.ok_or_else(unavailable)?;
    check()?;
    if key.len() != MASTER_KEY_BYTES {
        return Err(unavailable());
    }
    let mut hash = Sha256::new();
    hash.update(b"znet-sink/platform-vault-identity/v1\0");
    hash.update(key.as_slice());
    let key_id = format!("{:x}", hash.finalize());
    if let Some(identity) = identity {
        if identity.key_id != key_id {
            return Err(unavailable());
        }
    } else {
        if !create {
            return Err(unavailable());
        }
        let _history = super::uninstall::storage::writer(root)?;
        check()?;
        let value = KeyIdentity {
            schema_version: 1,
            key_id,
        };
        crate::services::atomic_file::write(
            &marker,
            &serde_json::to_vec(&value).map_err(|_| unavailable())?,
        )
        .map_err(|_| unavailable())?;
        set_private_permissions(&marker)?;
        let saved = read_identity(&marker)?;
        if saved.schema_version != 1 || saved.key_id != value.key_id {
            return Err(unavailable());
        }
    }
    check()?;
    Ok(key)
}

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

#[cfg(not(any(target_os = "android", target_os = "ios")))]
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
    let _history = super::uninstall::storage::writer(root)?;
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

#[cfg(any(all(target_os = "ios", debug_assertions), test))]
pub(super) fn exercise_platform_vault(
    root: &Path,
    provider: &KeyReader,
    cleanup: &dyn Fn() -> AppResult<()>,
    check: &dyn Fn() -> AppResult<()>,
) -> AppResult<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let creates = Arc::new(AtomicUsize::new(0));
    let counted: KeyReader = {
        let provider = provider.clone();
        let creates = creates.clone();
        Arc::new(move |create| {
            if create {
                creates.fetch_add(1, Ordering::SeqCst);
            }
            provider(create)
        })
    };
    let result = (|| {
        check()?;
        if counted(false)?.is_some() {
            return Err(unavailable());
        }
        let key = platform_key(root, &counted, true, check)?;
        let aad = b"znet-sink/native-vault-acceptance/v1";
        let encrypted = seal(
            key.as_slice(),
            aad,
            b"fixed non-sensitive native acceptance",
        )?;
        let restored = platform_key(root, &counted, false, check)?;
        if open(restored.as_slice(), aad, &encrypted)?.as_slice()
            != b"fixed non-sensitive native acceptance"
            || master_key_path(root).exists()
        {
            return Err(unavailable());
        }
        check()
    })();
    let cleaned = cleanup(); // Only the nonce-owned test account, even on failure.
    result?;
    cleaned?;
    check()?;
    if counted(false)?.is_some() {
        return Err(unavailable());
    }
    match platform_key(root, &counted, true, check) {
        Err(error)
            if error.code == "plugin_secure_store_unavailable"
                && creates.load(Ordering::SeqCst) == 1 =>
        {
            Ok(())
        }
        _ => Err(unavailable()),
    }
}

impl Host {
    fn vault_master(
        &self,
        root: &Path,
        create: bool,
        check: &dyn Fn() -> AppResult<()>,
    ) -> AppResult<Zeroizing<Vec<u8>>> {
        let provider = self.native_vault.lock().map_err(|_| unavailable())?.clone();
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            platform_key(root, &provider.ok_or_else(unavailable)?, create, check)
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            if let Some(provider) = provider {
                return platform_key(root, &provider, create, check);
            }
            check()?;
            master_key(root)
        }
    }
    #[cfg(test)]
    pub(super) fn vault_get(
        &self,
        plugin_id: &str,
        key: &str,
    ) -> AppResult<Option<Zeroizing<Vec<u8>>>> {
        self.vault_get_checked(plugin_id, key, &|| Ok(()))
    }
    pub(super) fn vault_get_checked(
        &self,
        plugin_id: &str,
        key: &str,
        check: &dyn Fn() -> AppResult<()>,
    ) -> AppResult<Option<Zeroizing<Vec<u8>>>> {
        check()?;
        if !safe_key(key) {
            return Err(AppError::invalid_argument("插件凭据键无效"));
        }
        let publisher = self.namespace_publisher(plugin_id)?;
        let root = self.root()?;
        let entries = load(&root, &publisher, plugin_id)?;
        let Some(value) = entries.0.get(key) else {
            return Ok(None);
        };
        let value = open(
            &self.vault_master(&root, false, check)?,
            &aad(&publisher, plugin_id, key),
            value,
        )?;
        check()?;
        Ok(Some(value))
    }

    #[cfg(test)]
    pub(super) fn vault_put(&self, plugin_id: &str, key: &str, value: &[u8]) -> AppResult<()> {
        self.vault_put_checked(plugin_id, key, value, &|| Ok(()))
    }
    pub(super) fn vault_put_checked(
        &self,
        plugin_id: &str,
        key: &str,
        value: &[u8],
        check: &dyn Fn() -> AppResult<()>,
    ) -> AppResult<()> {
        check()?;
        if !safe_key(key) || value.is_empty() || value.len() > MAX_VALUE_BYTES {
            return Err(AppError::invalid_argument("插件凭据键或内容无效"));
        }
        let publisher = self.namespace_publisher(plugin_id)?;
        let root = self.root()?;
        let mut entries = load(&root, &publisher, plugin_id)?;
        entries.0.insert(
            key.to_owned(),
            seal(
                &self.vault_master(&root, true, check)?,
                &aad(&publisher, plugin_id, key),
                value,
            )?,
        );
        check()?;
        save(&root, &publisher, plugin_id, &entries)
    }

    #[cfg(test)]
    pub(super) fn vault_delete(&self, plugin_id: &str, key: &str) -> AppResult<bool> {
        self.vault_delete_checked(plugin_id, key, &|| Ok(()))
    }
    pub(super) fn vault_delete_checked(
        &self,
        plugin_id: &str,
        key: &str,
        check: &dyn Fn() -> AppResult<()>,
    ) -> AppResult<bool> {
        check()?;
        if !safe_key(key) {
            return Err(AppError::invalid_argument("插件凭据键无效"));
        }
        let publisher = self.namespace_publisher(plugin_id)?;
        let root = self.root()?;
        let mut entries = load(&root, &publisher, plugin_id)?;
        let removed = entries.0.remove(key).is_some();
        check()?;
        save(&root, &publisher, plugin_id, &entries)?;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    #[test]
    fn authoritative_cached_snapshot_does_not_wait_for_inflight_native_secret_io() {
        use std::{sync::mpsc, thread, time::Duration};
        let (_root, host, manager) = super::super::tests::setup(false);
        let host = Arc::new(host);
        let manager = Arc::new(manager);
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let release_rx = Mutex::new(release_rx);
        *host.native_vault.lock().unwrap() = Some(Arc::new(move |_| {
            entered_tx.send(()).unwrap();
            release_rx.lock().unwrap().recv().unwrap();
            Ok(Some(Zeroizing::new(vec![3; 32])))
        }));
        let writer = {
            let host = host.clone();
            thread::spawn(move || {
                let _operation = host.operation_guard().unwrap();
                host.vault_put_checked(
                    "org.example.plugin",
                    "session/value",
                    b"non-sensitive test value",
                    &|| Ok(()),
                )
            })
        };
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let (snapshot_tx, snapshot_rx) = mpsc::sync_channel(1);
        let reader = {
            let host = host.clone();
            let manager = manager.clone();
            thread::spawn(move || snapshot_tx.send(host.snapshot(&manager).checked).unwrap())
        };
        let observed = snapshot_rx.recv_timeout(Duration::from_secs(2));
        release_tx.send(()).unwrap();
        writer.join().unwrap().unwrap();
        reader.join().unwrap();
        assert_eq!(observed.unwrap(), true);
    }
    #[test]
    fn fixed_native_vault_acceptance_uses_real_cipher_and_always_cleans_its_key() {
        let root = tempfile::tempdir().unwrap();
        let value = Arc::new(Mutex::new(None::<Vec<u8>>));
        let provider: KeyReader = {
            let value = value.clone();
            Arc::new(move |create| {
                let mut key = value.lock().unwrap();
                if create && key.is_none() {
                    *key = Some(vec![7; 32]);
                }
                Ok(key.clone().map(Zeroizing::new))
            })
        };
        let cleanup = || {
            *value.lock().unwrap() = None;
            Ok(())
        };
        exercise_platform_vault(root.path(), &provider, &cleanup, &|| Ok(())).unwrap();
        assert!(value.lock().unwrap().is_none());
        assert!(!master_key_path(root.path()).exists());
        let failures = AtomicUsize::new(0);
        let failing: KeyReader = Arc::new(|_| Err(unavailable()));
        assert!(exercise_platform_vault(
            root.path(),
            &failing,
            &|| {
                failures.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            &|| Ok(())
        )
        .is_err());
        assert_eq!(failures.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn platform_key_never_replaces_missing_or_different_material() {
        let root = tempfile::tempdir().unwrap();
        let key = Arc::new(Mutex::new(Some(vec![9u8; 32])));
        let creates = Arc::new(AtomicUsize::new(0));
        let provider: KeyReader = {
            let key = key.clone();
            let creates = creates.clone();
            Arc::new(move |create| {
                if create {
                    creates.fetch_add(1, Ordering::SeqCst);
                }
                Ok(key.lock().unwrap().clone().map(Zeroizing::new))
            })
        };
        assert!(platform_key(root.path(), &provider, false, &|| Ok(())).is_err());
        assert_eq!(creates.load(Ordering::SeqCst), 0);
        assert_eq!(
            platform_key(root.path(), &provider, true, &|| Ok(()))
                .unwrap()
                .as_slice(),
            &[9; 32]
        );
        assert!(!master_key_path(root.path()).exists());
        let marker = root.path().join("vault.platform-identity.v1.json");
        let original = fs::read(&marker).unwrap();
        assert!(original.len() <= 256);
        assert_eq!(
            platform_key(root.path(), &provider, false, &|| Ok(()))
                .unwrap()
                .len(),
            32
        );
        *key.lock().unwrap() = None;
        assert!(platform_key(root.path(), &provider, true, &|| Ok(())).is_err());
        *key.lock().unwrap() = Some(vec![8; 32]);
        assert!(platform_key(root.path(), &provider, true, &|| Ok(())).is_err());
        assert_eq!(fs::read(&marker).unwrap(), original);
        assert_eq!(creates.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn revoked_native_key_return_cannot_publish_identity_or_use_secret() {
        let root = tempfile::tempdir().unwrap();
        let allowed = Arc::new(AtomicBool::new(true));
        let provider: KeyReader = {
            let allowed = allowed.clone();
            Arc::new(move |_| {
                allowed.store(false, Ordering::SeqCst);
                Ok(Some(Zeroizing::new(vec![4; 32])))
            })
        };
        let check = || {
            if allowed.load(Ordering::SeqCst) {
                Ok(())
            } else {
                Err(stale())
            }
        };
        assert!(platform_key(root.path(), &provider, true, &check).is_err());
        assert!(!root.path().join("vault.platform-identity.v1.json").exists());
        assert!(!master_key_path(root.path()).exists());
    }

    #[test]
    fn malformed_oversized_or_legacy_identity_does_not_reach_platform_store() {
        let root = tempfile::tempdir().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let provider: KeyReader = {
            let calls = calls.clone();
            Arc::new(move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(Some(Zeroizing::new(vec![1; 32])))
            })
        };
        let marker = root.path().join("vault.platform-identity.v1.json");
        for bytes in [b"{}".to_vec(), vec![b'x'; 257]] {
            fs::write(&marker, bytes).unwrap();
            assert!(platform_key(root.path(), &provider, true, &|| Ok(())).is_err());
        }
        fs::remove_file(&marker).unwrap();
        fs::write(master_key_path(root.path()), [1; 32]).unwrap();
        assert!(platform_key(root.path(), &provider, true, &|| Ok(())).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[cfg(unix)]
    #[test]
    fn platform_identity_never_follows_a_symlink() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("other");
        fs::write(&target, b"{}").unwrap();
        std::os::unix::fs::symlink(&target, root.path().join("vault.platform-identity.v1.json"))
            .unwrap();
        let provider: KeyReader =
            Arc::new(|_| panic!("untrusted identity must not invoke native store"));
        assert!(platform_key(root.path(), &provider, true, &|| Ok(())).is_err());
        assert_eq!(fs::read(target).unwrap(), b"{}");
    }

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
