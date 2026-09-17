//! Host-owned installation intent and permission consent. Plugin business data
//! lives in the separate namespace store and is never interpreted here.

use super::*;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};

const MAX_BYTES: usize = 256 * 1024;

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    #[serde(default = "schema_version")]
    schema_version: u32,
    #[serde(default)]
    plugins: BTreeMap<String, PluginRecord>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginRecord {
    publisher_fingerprint: String,
    #[serde(default)]
    components: BTreeMap<String, ComponentConsent>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalTrust {
    #[serde(default = "schema_version")]
    schema_version: u32,
    #[serde(default)]
    plugins: BTreeMap<String, Registration>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ComponentConsent {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub grants: BTreeSet<Request>,
    /// The complete declaration the user reviewed. Added permissions or wider
    /// scopes never inherit consent merely because a package was upgraded.
    #[serde(default)]
    pub approved_declaration: BTreeSet<Request>,
}

fn schema_version() -> u32 {
    1
}

fn path(root: &Path) -> std::path::PathBuf {
    root.join("registry.json")
}

fn local_trust_path(root: &Path) -> std::path::PathBuf {
    root.join("local-trust.json")
}

fn reject_symlink(path: &Path) -> AppResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(AppError::invalid_argument("插件状态路径不能是符号链接"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io::failure(error)),
    }
}

fn load(root: &Path) -> AppResult<Registry> {
    fs::create_dir_all(root).map_err(io::failure)?;
    reject_symlink(root)?;
    let file_path = path(root);
    reject_symlink(&file_path)?;
    let file = match File::open(file_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Registry {
                schema_version: schema_version(),
                ..Registry::default()
            });
        }
        Err(error) => return Err(io::failure(error)),
    };
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(io::failure)?;
    if bytes.len() > MAX_BYTES {
        return Err(AppError::invalid_argument("插件状态文件超过大小限制"));
    }
    let registry: Registry = serde_json::from_slice(&bytes).map_err(io::failure)?;
    if registry.schema_version != schema_version()
        || registry.plugins.len() > 64
        || registry
            .plugins
            .values()
            .any(|plugin| plugin.components.len() > 32)
    {
        return Err(AppError::invalid_argument("插件状态文件格式无效"));
    }
    Ok(registry)
}

fn save(root: &Path, registry: &Registry) -> AppResult<()> {
    let bytes = serde_json::to_vec(registry).map_err(io::failure)?;
    if bytes.len() > MAX_BYTES {
        return Err(AppError::invalid_argument("插件状态超过容量限制"));
    }
    let mut temporary = tempfile::NamedTempFile::new_in(root).map_err(io::failure)?;
    temporary.write_all(&bytes).map_err(io::failure)?;
    temporary.as_file().sync_all().map_err(io::failure)?;
    temporary
        .persist(path(root))
        .map_err(|error| io::failure(error.error))?;
    #[cfg(unix)]
    File::open(root)
        .and_then(|directory| directory.sync_all())
        .map_err(io::failure)?;
    Ok(())
}

pub(super) fn consent(
    root: &Path,
    publisher_fingerprint: &str,
    plugin_id: &str,
    component_id: &str,
) -> AppResult<ComponentConsent> {
    let registry = load(root)?;
    Ok(registry
        .plugins
        .get(plugin_id)
        .filter(|plugin| plugin.publisher_fingerprint == publisher_fingerprint)
        .and_then(|plugin| plugin.components.get(component_id))
        .cloned()
        .unwrap_or_default())
}

pub(super) fn approve(
    root: &Path,
    publisher_fingerprint: &str,
    plugin_id: &str,
    component_id: &str,
    grants: BTreeSet<Request>,
    declaration: BTreeSet<Request>,
) -> AppResult<()> {
    let mut registry = load(root)?;
    let plugin = registry
        .plugins
        .entry(plugin_id.into())
        .or_insert_with(|| PluginRecord {
            publisher_fingerprint: publisher_fingerprint.into(),
            components: BTreeMap::new(),
        });
    if plugin.publisher_fingerprint != publisher_fingerprint {
        return Err(AppError::invalid_argument(
            "插件发布者身份已变化，不能继承旧权限",
        ));
    }
    plugin.components.insert(
        component_id.into(),
        ComponentConsent {
            enabled: true,
            grants,
            approved_declaration: declaration,
        },
    );
    save(root, &registry)
}

pub(super) fn set_enabled(
    root: &Path,
    publisher_fingerprint: &str,
    plugin_id: &str,
    component_id: &str,
    enabled: bool,
) -> AppResult<()> {
    let mut registry = load(root)?;
    let Some(plugin) = registry.plugins.get_mut(plugin_id) else {
        return if enabled {
            Err(AppError::invalid_argument("请先确认插件权限"))
        } else {
            Ok(())
        };
    };
    if plugin.publisher_fingerprint != publisher_fingerprint {
        return Err(AppError::invalid_argument("插件发布者身份不匹配"));
    }
    let Some(component) = plugin.components.get_mut(component_id) else {
        return if enabled {
            Err(AppError::invalid_argument("请先确认插件权限"))
        } else {
            Ok(())
        };
    };
    component.enabled = enabled;
    save(root, &registry)
}

pub(super) fn revoke(
    root: &Path,
    publisher_fingerprint: &str,
    plugin_id: &str,
    component_id: &str,
) -> AppResult<()> {
    let mut registry = load(root)?;
    if let Some(plugin) = registry
        .plugins
        .get_mut(plugin_id)
        .filter(|plugin| plugin.publisher_fingerprint == publisher_fingerprint)
    {
        plugin.components.remove(component_id);
        if plugin.components.is_empty() {
            registry.plugins.remove(plugin_id);
        }
        save(root, &registry)?;
    }
    Ok(())
}

pub(super) fn remove_plugin(root: &Path, plugin_id: &str) -> AppResult<()> {
    let mut registry = load(root)?;
    if registry.plugins.remove(plugin_id).is_some() {
        save(root, &registry)?;
    }
    Ok(())
}

pub(super) fn save_directory(root: &Path, directory: &Directory) -> AppResult<()> {
    let bytes = serde_json::to_vec(directory).map_err(io::failure)?;
    if bytes.len() > 1024 * 1024 {
        return Err(AppError::invalid_argument("插件登记缓存超过大小限制"));
    }
    let mut temporary = tempfile::NamedTempFile::new_in(root).map_err(io::failure)?;
    temporary.write_all(&bytes).map_err(io::failure)?;
    temporary.as_file().sync_all().map_err(io::failure)?;
    temporary
        .persist(root.join("directory-cache.json"))
        .map_err(|error| io::failure(error.error))?;
    Ok(())
}

pub(super) fn load_directory(root: &Path) -> AppResult<Option<Directory>> {
    let path = root.join("directory-cache.json");
    reject_symlink(&path)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io::failure(error)),
    };
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(io::failure)?;
    if bytes.len() > 1024 * 1024 {
        return Err(AppError::invalid_argument("插件登记缓存超过大小限制"));
    }
    Directory::parse(&bytes).map(Some).map_err(io::failure)
}

fn load_local_trust(root: &Path) -> AppResult<LocalTrust> {
    fs::create_dir_all(root).map_err(io::failure)?;
    reject_symlink(root)?;
    let path = local_trust_path(root);
    reject_symlink(&path)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LocalTrust {
                schema_version: schema_version(),
                ..LocalTrust::default()
            });
        }
        Err(error) => return Err(io::failure(error)),
    };
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(io::failure)?;
    if bytes.len() > MAX_BYTES {
        return Err(AppError::invalid_argument("本地插件信任记录超过大小限制"));
    }
    let trust: LocalTrust = serde_json::from_slice(&bytes).map_err(io::failure)?;
    if trust.schema_version != schema_version() || trust.plugins.len() > 64 {
        return Err(AppError::invalid_argument("本地插件信任记录格式无效"));
    }
    for (id, registration) in &trust.plugins {
        if id != &registration.id {
            return Err(AppError::invalid_argument("本地插件信任记录身份不一致"));
        }
        registration.validate().map_err(io::failure)?;
    }
    Ok(trust)
}

fn save_local_trust(root: &Path, trust: &LocalTrust) -> AppResult<()> {
    let bytes = serde_json::to_vec(trust).map_err(io::failure)?;
    if bytes.len() > MAX_BYTES || trust.plugins.len() > 64 {
        return Err(AppError::invalid_argument("本地插件信任记录超过大小限制"));
    }
    let mut temporary = tempfile::NamedTempFile::new_in(root).map_err(io::failure)?;
    temporary.write_all(&bytes).map_err(io::failure)?;
    temporary.as_file().sync_all().map_err(io::failure)?;
    temporary
        .persist(local_trust_path(root))
        .map_err(|error| io::failure(error.error))?;
    Ok(())
}

pub(super) fn save_local_registration(root: &Path, registration: Registration) -> AppResult<()> {
    registration.validate().map_err(io::failure)?;
    let mut trust = load_local_trust(root)?;
    trust.plugins.insert(registration.id.clone(), registration);
    save_local_trust(root, &trust)
}

pub(super) fn remove_local_registration(root: &Path, plugin_id: &str) -> AppResult<()> {
    let mut trust = load_local_trust(root)?;
    if trust.plugins.remove(plugin_id).is_some() {
        save_local_trust(root, &trust)?;
    }
    Ok(())
}

/// Merge public marketplace metadata with user-approved local trust. Local
/// records win for an installed package because they may contain declarations
/// that were explicitly approved without ever being published online.
pub(super) fn effective_directory(
    root: &Path,
    central: Option<Directory>,
) -> AppResult<Option<Directory>> {
    let trust = load_local_trust(root)?;
    if central.is_none() && trust.plugins.is_empty() {
        return Ok(None);
    }
    let mut directory = central.unwrap_or(Directory {
        snapshot_version: None,
        schema_version: 2,
        host: "znet-sink".into(),
        plugins: Vec::new(),
    });
    for registration in trust.plugins.into_values() {
        directory
            .plugins
            .retain(|value| value.id != registration.id);
        directory.plugins.push(registration);
    }
    Ok(Some(directory))
}
