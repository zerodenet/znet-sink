//! Opaque, host-isolated plugin data. The host enforces ownership, quotas and
//! atomicity but deliberately does not interpret or decrypt plugin values.

use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_KEYS: usize = 256;
const MAX_VALUE_BYTES: usize = 128 * 1024;
const MAX_STATE_BYTES: usize = 2 * 1024 * 1024;
const MAX_CACHE_BYTES: usize = 4 * 1024 * 1024;
const MAX_RUNTIME_STATE_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Area {
    State,
    Cache,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    #[serde(default = "initial_schema")]
    state_schema: u32,
    #[serde(default = "initial_schema")]
    cache_schema: u32,
}

fn initial_schema() -> u32 {
    1
}

impl Metadata {
    fn version(&self, area: Area) -> u32 {
        match area {
            Area::State => self.state_schema,
            Area::Cache => self.cache_schema,
        }
    }
    fn set_version(&mut self, area: Area, value: u32) {
        match area {
            Area::State => self.state_schema = value,
            Area::Cache => self.cache_schema = value,
        }
    }
}

impl Area {
    fn name(self) -> &'static str {
        match self {
            Self::State => "state",
            Self::Cache => "cache",
        }
    }
    fn limit(self) -> usize {
        match self {
            Self::State => MAX_STATE_BYTES,
            Self::Cache => MAX_CACHE_BYTES,
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(transparent)]
struct Values(BTreeMap<String, String>);

fn safe_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

fn safe_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value.split('/').count() <= 6
        && value
            .split('/')
            .all(|part| part != "." && part != ".." && safe_identity(part))
}

fn reject_symlink(path: &Path) -> AppResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(AppError::invalid_argument("插件命名空间不能包含符号链接"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io::failure(error)),
    }
}

pub(super) fn directory(root: &Path, publisher: &str, plugin_id: &str) -> AppResult<PathBuf> {
    if !safe_identity(publisher) || !safe_identity(plugin_id) {
        return Err(AppError::invalid_argument("插件命名空间身份无效"));
    }
    let namespaces = root.join("namespaces");
    let publisher_dir = namespaces.join(publisher);
    let plugin_dir = publisher_dir.join(plugin_id);
    for path in [
        root,
        namespaces.as_path(),
        publisher_dir.as_path(),
        plugin_dir.as_path(),
    ] {
        reject_symlink(path)?;
        fs::create_dir_all(path).map_err(io::failure)?;
    }
    Ok(plugin_dir)
}

fn file(root: &Path, publisher: &str, plugin_id: &str, area: Area) -> AppResult<PathBuf> {
    Ok(directory(root, publisher, plugin_id)?.join(format!("{}.json", area.name())))
}

fn metadata_path(root: &Path, publisher: &str, plugin_id: &str) -> AppResult<PathBuf> {
    Ok(directory(root, publisher, plugin_id)?.join("metadata.json"))
}

fn load_metadata(root: &Path, publisher: &str, plugin_id: &str) -> AppResult<Metadata> {
    let path = metadata_path(root, publisher, plugin_id)?;
    reject_symlink(&path)?;
    match fs::read(path) {
        Ok(bytes) if bytes.len() <= 4096 => serde_json::from_slice(&bytes).map_err(io::failure),
        Ok(_) => Err(AppError::invalid_argument("插件命名空间元数据超过容量限制")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Metadata {
            state_schema: 1,
            cache_schema: 1,
        }),
        Err(error) => Err(io::failure(error)),
    }
}

fn save_metadata(
    root: &Path,
    publisher: &str,
    plugin_id: &str,
    metadata: &Metadata,
) -> AppResult<()> {
    if metadata.state_schema == 0 || metadata.cache_schema == 0 {
        return Err(AppError::invalid_argument("插件命名空间版本无效"));
    }
    let directory = directory(root, publisher, plugin_id)?;
    let bytes = serde_json::to_vec(metadata).map_err(io::failure)?;
    let mut temporary = tempfile::NamedTempFile::new_in(&directory).map_err(io::failure)?;
    temporary.write_all(&bytes).map_err(io::failure)?;
    temporary.as_file().sync_all().map_err(io::failure)?;
    temporary
        .persist(metadata_path(root, publisher, plugin_id)?)
        .map_err(|error| io::failure(error.error))?;
    Ok(())
}

fn load(root: &Path, publisher: &str, plugin_id: &str, area: Area) -> AppResult<Values> {
    let path = file(root, publisher, plugin_id, area)?;
    reject_symlink(&path)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Values::default()),
        Err(error) => return Err(io::failure(error)),
    };
    let mut bytes = Vec::new();
    file.take((area.limit() + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(io::failure)?;
    if bytes.len() > area.limit() {
        return Err(AppError::invalid_argument("插件命名空间超过容量限制"));
    }
    let values: Values = serde_json::from_slice(&bytes).map_err(io::failure)?;
    validate(&values, area)?;
    Ok(values)
}

fn validate(values: &Values, area: Area) -> AppResult<usize> {
    if values.0.len() > MAX_KEYS || values.0.keys().any(|key| !safe_key(key)) {
        return Err(AppError::invalid_argument("插件命名空间键无效"));
    }
    let mut total = 0usize;
    for value in values.0.values() {
        let decoded = STANDARD
            .decode(value)
            .map_err(|_| AppError::invalid_argument("插件状态不是有效的 Base64 数据"))?;
        if decoded.len() > MAX_VALUE_BYTES {
            return Err(AppError::invalid_argument("插件状态单项超过大小限制"));
        }
        total = total.saturating_add(decoded.len());
    }
    if total > area.limit() {
        return Err(AppError::invalid_argument("插件命名空间超过容量限制"));
    }
    Ok(total)
}

fn save(
    root: &Path,
    publisher: &str,
    plugin_id: &str,
    area: Area,
    values: &Values,
) -> AppResult<()> {
    validate(values, area)?;
    let directory = directory(root, publisher, plugin_id)?;
    let bytes = serde_json::to_vec(values).map_err(io::failure)?;
    if bytes.len() > area.limit() {
        return Err(AppError::invalid_argument("插件命名空间超过容量限制"));
    }
    let mut temporary = tempfile::NamedTempFile::new_in(&directory).map_err(io::failure)?;
    temporary.write_all(&bytes).map_err(io::failure)?;
    temporary.as_file().sync_all().map_err(io::failure)?;
    temporary
        .persist(file(root, publisher, plugin_id, area)?)
        .map_err(|error| io::failure(error.error))?;
    #[cfg(unix)]
    File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(io::failure)?;
    Ok(())
}

pub(super) fn get(
    root: &Path,
    publisher: &str,
    plugin_id: &str,
    area: Area,
    key: &str,
) -> AppResult<Option<String>> {
    if !safe_key(key) {
        return Err(AppError::invalid_argument("插件状态键无效"));
    }
    Ok(load(root, publisher, plugin_id, area)?.0.get(key).cloned())
}

pub(super) fn put(
    root: &Path,
    publisher: &str,
    plugin_id: &str,
    area: Area,
    key: String,
    value: String,
) -> AppResult<()> {
    if !safe_key(&key) {
        return Err(AppError::invalid_argument("插件状态键无效"));
    }
    let mut values = load(root, publisher, plugin_id, area)?;
    values.0.insert(key, value);
    save(root, publisher, plugin_id, area, &values)
}

pub(super) fn delete(
    root: &Path,
    publisher: &str,
    plugin_id: &str,
    area: Area,
    key: &str,
) -> AppResult<()> {
    if !safe_key(key) {
        return Err(AppError::invalid_argument("插件状态键无效"));
    }
    let mut values = load(root, publisher, plugin_id, area)?;
    if values.0.remove(key).is_some() {
        save(root, publisher, plugin_id, area, &values)?;
    }
    Ok(())
}

pub(super) fn list(
    root: &Path,
    publisher: &str,
    plugin_id: &str,
    area: Area,
) -> AppResult<Vec<String>> {
    Ok(load(root, publisher, plugin_id, area)?
        .0
        .into_keys()
        .collect())
}

pub(super) fn export(
    root: &Path,
    publisher: &str,
    plugin_id: &str,
    area: Area,
) -> AppResult<(u32, BTreeMap<String, String>)> {
    let version = load_metadata(root, publisher, plugin_id)?.version(area);
    Ok((version, load(root, publisher, plugin_id, area)?.0))
}

pub(super) fn clear(root: &Path, publisher: &str, plugin_id: &str, area: Area) -> AppResult<()> {
    save(root, publisher, plugin_id, area, &Values::default())
}

pub(super) fn migrate(
    root: &Path,
    publisher: &str,
    plugin_id: &str,
    area: Area,
    from: u32,
    to: u32,
    values: BTreeMap<String, String>,
) -> AppResult<()> {
    if from == 0 || to <= from || to > from.saturating_add(1) {
        return Err(AppError::invalid_argument("插件存储迁移必须逐版本向前执行"));
    }
    let mut metadata = load_metadata(root, publisher, plugin_id)?;
    if metadata.version(area) != from {
        return Err(AppError::conflict(
            "plugin_storage",
            area.name(),
            "插件存储版本已变化，请重新读取后迁移",
        ));
    }
    let values = Values(values);
    validate(&values, area)?;
    save(root, publisher, plugin_id, area, &values)?;
    metadata.set_version(area, to);
    if let Err(error) = save_metadata(root, publisher, plugin_id, &metadata) {
        return Err(AppError {
            code: "plugin_storage_migration_uncertain",
            message: format!("插件存储内容已写入，但版本标记保存失败：{}", error.message),
            details: None,
        });
    }
    Ok(())
}

pub(super) fn runtime_state(
    root: &Path,
    publisher: &str,
    plugin_id: &str,
) -> AppResult<BTreeMap<String, String>> {
    let values = load(root, publisher, plugin_id, Area::State)?;
    let bytes = serde_json::to_vec(&values.0).map_err(io::failure)?;
    if bytes.len() > MAX_RUNTIME_STATE_BYTES {
        return Err(AppError::invalid_argument(
            "插件后台运行状态超过 8 KiB，请由管理页面缩减运行态数据",
        ));
    }
    Ok(values.0)
}

pub(super) fn apply_runtime_updates(
    root: &Path,
    publisher: &str,
    plugin_id: &str,
    updates: BTreeMap<String, Option<String>>,
) -> AppResult<()> {
    if updates.len() > 32 || updates.keys().any(|key| !safe_key(key)) {
        return Err(AppError::invalid_argument("插件运行态更新无效"));
    }
    let mut values = load(root, publisher, plugin_id, Area::State)?;
    for (key, value) in updates {
        match value {
            Some(value) => {
                values.0.insert(key, value);
            }
            None => {
                values.0.remove(&key);
            }
        }
    }
    save(root, publisher, plugin_id, Area::State, &values)
}

pub(super) fn remove_plugin(root: &Path, publisher: &str, plugin_id: &str) -> AppResult<()> {
    let path = directory(root, publisher, plugin_id)?;
    reject_symlink(&path)?;
    if path.exists() {
        fs::remove_dir_all(path).map_err(io::failure)?;
    }
    Ok(())
}

impl Host {
    pub(super) fn namespace_publisher(&self, plugin_id: &str) -> AppResult<String> {
        self.state
            .lock()
            .unwrap()
            .loaded
            .values()
            .find(|loaded| loaded.component.manifest().plugin_id == plugin_id)
            .map(|loaded| loaded.publisher_fingerprint.clone())
            .ok_or_else(|| AppError::invalid_argument("插件未安装或尚未完成校验"))
    }

    pub fn storage_get(&self, plugin_id: &str, area: Area, key: &str) -> AppResult<Option<String>> {
        let _operation = self.operation.lock().unwrap();
        let publisher = self.namespace_publisher(plugin_id)?;
        get(&self.root()?, &publisher, plugin_id, area, key)
    }

    pub fn storage_put(
        &self,
        plugin_id: &str,
        area: Area,
        key: String,
        value: String,
    ) -> AppResult<()> {
        let _operation = self.operation.lock().unwrap();
        let publisher = self.namespace_publisher(plugin_id)?;
        put(&self.root()?, &publisher, plugin_id, area, key, value)
    }

    pub fn storage_delete(&self, plugin_id: &str, area: Area, key: &str) -> AppResult<()> {
        let _operation = self.operation.lock().unwrap();
        let publisher = self.namespace_publisher(plugin_id)?;
        delete(&self.root()?, &publisher, plugin_id, area, key)
    }

    pub fn storage_list(&self, plugin_id: &str, area: Area) -> AppResult<Vec<String>> {
        let _operation = self.operation.lock().unwrap();
        let publisher = self.namespace_publisher(plugin_id)?;
        list(&self.root()?, &publisher, plugin_id, area)
    }

    pub fn storage_export(
        &self,
        plugin_id: &str,
        area: Area,
    ) -> AppResult<(u32, BTreeMap<String, String>)> {
        let _operation = self.operation.lock().unwrap();
        let publisher = self.namespace_publisher(plugin_id)?;
        export(&self.root()?, &publisher, plugin_id, area)
    }

    pub fn storage_clear(&self, plugin_id: &str, area: Area) -> AppResult<()> {
        let _operation = self.operation.lock().unwrap();
        let publisher = self.namespace_publisher(plugin_id)?;
        clear(&self.root()?, &publisher, plugin_id, area)
    }

    pub fn storage_migrate(
        &self,
        plugin_id: &str,
        area: Area,
        from: u32,
        to: u32,
        values: BTreeMap<String, String>,
    ) -> AppResult<()> {
        let _operation = self.operation.lock().unwrap();
        let publisher = self.namespace_publisher(plugin_id)?;
        migrate(&self.root()?, &publisher, plugin_id, area, from, to, values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_is_versioned_bounded_and_exportable() {
        let root = tempfile::tempdir().unwrap();
        let mut values = BTreeMap::new();
        values.insert("session/envelope".into(), STANDARD.encode(b"opaque"));
        migrate(
            root.path(),
            "publisher",
            "org.example.plugin",
            Area::State,
            1,
            2,
            values.clone(),
        )
        .unwrap();
        let (version, exported) =
            export(root.path(), "publisher", "org.example.plugin", Area::State).unwrap();
        assert_eq!(version, 2);
        assert_eq!(exported, values);
        assert!(migrate(
            root.path(),
            "publisher",
            "org.example.plugin",
            Area::State,
            1,
            2,
            BTreeMap::new()
        )
        .is_err());
        assert!(migrate(
            root.path(),
            "publisher",
            "org.example.plugin",
            Area::State,
            2,
            4,
            BTreeMap::new()
        )
        .is_err());
    }
}
