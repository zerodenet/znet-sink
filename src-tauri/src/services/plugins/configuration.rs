use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const FILE_NAME: &str = "configurations.json";
const MAX_BYTES: usize = 128 * 1024;

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    #[serde(default = "schema_version")]
    schema_version: u32,
    #[serde(default)]
    components: BTreeMap<String, BTreeMap<String, String>>,
}

fn schema_version() -> u32 {
    1
}

fn path(root: &Path) -> PathBuf {
    root.join(FILE_NAME)
}

fn read(root: &Path) -> AppResult<State> {
    let path = path(root);
    if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(AppError::invalid_argument("插件配置文件不能是符号链接"));
    }
    let mut file = match fs::File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(State {
                schema_version: schema_version(),
                ..State::default()
            });
        }
        Err(error) => return Err(AppError::internal(format!("无法读取插件配置：{error}"))),
    };
    let mut bytes = Vec::new();
    file.by_ref()
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| AppError::internal(format!("无法读取插件配置：{error}")))?;
    if bytes.len() > MAX_BYTES {
        return Err(AppError::invalid_argument("插件配置文件超过大小限制"));
    }
    let state: State = serde_json::from_slice(&bytes)
        .map_err(|_| AppError::invalid_argument("插件配置文件格式无效"))?;
    if state.schema_version != schema_version() || state.components.len() > 128 {
        return Err(AppError::invalid_argument("插件配置文件版本或条目无效"));
    }
    Ok(state)
}

fn write(root: &Path, state: &State) -> AppResult<()> {
    let bytes = serde_json::to_vec(state)
        .map_err(|error| AppError::internal(format!("无法序列化插件配置：{error}")))?;
    if bytes.len() > MAX_BYTES {
        return Err(AppError::invalid_argument("插件配置超过大小限制"));
    }
    crate::services::atomic_file::write(&path(root), &bytes)
        .map_err(|error| AppError::internal(format!("无法保存插件配置：{error}")))?;
    Ok(())
}

pub(super) fn values(root: &Path, key: &str) -> AppResult<BTreeMap<String, String>> {
    Ok(read(root)?.components.remove(key).unwrap_or_default())
}

pub(super) fn save(root: &Path, key: &str, values: BTreeMap<String, String>) -> AppResult<()> {
    let mut state = read(root)?;
    state.components.insert(key.to_owned(), values);
    write(root, &state)
}

pub(super) fn remove_plugin(root: &Path, plugin_id: &str) -> AppResult<()> {
    let mut state = read(root)?;
    let prefix = format!("{plugin_id}/");
    state.components.retain(|key, _| !key.starts_with(&prefix));
    write(root, &state)
}
