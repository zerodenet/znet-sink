//! Fixed per-file candidates kept only in the short-lived host plan. Persistent
//! records contain hashes and receipts, never these opaque values.
use super::*;
use serde_json::{json, Value};
#[derive(Clone)]
pub(super) struct FilePlan {
    pub key: String,
    pub kind: String,
    pub name: String,
    pub relative: PathBuf,
    pub before: String,
    pub candidate: Option<Vec<u8>>,
    pub namespace: bool,
    pub directory: bool,
}
fn object<'a>(v: &'a mut Value, key: &str) -> AppResult<&'a mut serde_json::Map<String, Value>> {
    v.get_mut(key)
        .and_then(Value::as_object_mut)
        .ok_or_else(storage::error)
}
fn load(
    root: &Path,
    name: &str,
    max: usize,
    collection: &str,
) -> AppResult<Option<(Value, Vec<u8>)>> {
    let Some(bytes) = storage::read(&root.join(name), max)? else {
        return Ok(None);
    };
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| storage::error())?;
    if value.as_object().is_none_or(|o| o.len() != 2)
        || value["schema_version"] != json!(1)
        || !value[collection].is_object()
    {
        return Err(storage::error());
    }
    Ok(Some((value, bytes)))
}
fn push(
    out: &mut Vec<FilePlan>,
    name: &str,
    label: &str,
    before: &[u8],
    candidate: &Value,
) -> AppResult<()> {
    let next = serde_json::to_vec(candidate).map_err(|_| storage::error())?;
    out.push(FilePlan {
        key: name.into(),
        kind: "plugin_record".into(),
        name: label.into(),
        relative: name.into(),
        before: storage::hash(before),
        candidate: Some(next),
        namespace: false,
        directory: false,
    });
    Ok(())
}
pub(super) fn prepare(
    root: &Path,
    registration: &Registration,
    publisher: &str,
    components: &[String],
) -> AppResult<(Vec<FilePlan>, Vec<String>)> {
    let plugin = &registration.id;
    let keys: BTreeSet<_> = components.iter().map(|c| format!("{plugin}/{c}")).collect();
    let mut result = vec![];
    let mut blockers = vec![];
    if let Some((mut value, bytes)) = load(root, "configurations.json", 128 * 1024, "components")? {
        let entries = object(&mut value, "components")?;
        if entries
            .keys()
            .any(|k| k.starts_with(&format!("{plugin}/")) && !keys.contains(k))
        {
            blockers.push("component_record_identity_unknown".into());
        }
        let before = entries.len();
        entries.retain(|k, _| !keys.contains(k));
        if entries.len() != before {
            push(
                &mut result,
                "configurations.json",
                "原组件配置记录",
                &bytes,
                &value,
            )?;
        }
    }
    if let Some((mut value, bytes)) = load(root, "registry.json", 256 * 1024, "plugins")? {
        let entries = object(&mut value, "plugins")?;
        if let Some(v) = entries.get(plugin) {
            if v["publisher_fingerprint"].as_str() != Some(publisher)
                || v["components"]
                    .as_object()
                    .is_none_or(|m| m.keys().any(|k| !components.contains(k)))
            {
                blockers.push("consent_owner_unknown".into())
            } else {
                entries.remove(plugin);
                push(
                    &mut result,
                    "registry.json",
                    "原发布者权限与启用记录",
                    &bytes,
                    &value,
                )?;
            }
        }
    }
    if let Some((mut value, bytes)) = load(root, "local-trust.json", 256 * 1024, "plugins")? {
        let entries = object(&mut value, "plugins")?;
        if let Some(v) = entries.get(plugin) {
            let exact = serde_json::from_value::<Registration>(v.clone())
                .is_ok_and(|r| digest(&r).ok() == digest(registration).ok());
            if !exact {
                blockers.push("publisher_registration_changed".into())
            } else {
                entries.remove(plugin);
                push(
                    &mut result,
                    "local-trust.json",
                    "原本地发布者登记",
                    &bytes,
                    &value,
                )?;
            }
        }
    }
    if let Some((mut value, bytes)) = load(root, "schedules.json", 256 * 1024, "tasks")? {
        let entries = object(&mut value, "tasks")?;
        let mut selected = vec![];
        for (key, value) in entries.iter() {
            let task: super::super::scheduler::Task =
                serde_json::from_value(value.clone()).map_err(|_| storage::error())?;
            if *key != format!("{}/{}/{}", task.plugin_id, task.component_id, task.task_id) {
                return Err(storage::error());
            }
            if task.plugin_id == *plugin {
                if !components.contains(&task.component_id) {
                    blockers.push("schedule_owner_unknown".into())
                } else {
                    selected.push(key.clone())
                }
            }
        }
        if !selected.is_empty() {
            for key in selected {
                entries.remove(&key);
            }
            push(
                &mut result,
                "schedules.json",
                "原组件后台任务记录",
                &bytes,
                &value,
            )?;
        }
    }
    // Each known file and the exact now-empty directory is a separate target.
    // Unknown files, links and nested directories are never recursively removed.
    let relative = PathBuf::from("namespaces").join(publisher).join(plugin);
    let full = root.join(&relative);
    if full.try_exists().map_err(|_| storage::error())? {
        for p in [
            root.join("namespaces"),
            root.join("namespaces").join(publisher),
            full.clone(),
        ] {
            storage::ordinary(&p, true)?;
        }
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&full).map_err(|_| storage::error())? {
            let entry = entry.map_err(|_| storage::error())?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| storage::error())?;
            let limit = match name.as_str() {
                "state.json" => 2 * 1024 * 1024,
                "cache.json" => 4 * 1024 * 1024,
                "metadata.json" => 4096,
                "vault.json" => 4 * 1024 * 1024,
                _ => {
                    blockers.push("namespace_unrecognized_material".into());
                    continue;
                }
            };
            let bytes = storage::read(&entry.path(), limit)?.ok_or_else(storage::error)?;
            names.push(name.clone());
            result.push(FilePlan {
                key: format!("namespace/{name}"),
                kind: "namespace_file".into(),
                name: match name.as_str() {
                    "state.json" => "原插件独立状态",
                    "cache.json" => "原插件独立缓存",
                    "vault.json" => "原插件加密凭据库",
                    _ => "原插件独立元数据",
                }
                .into(),
                relative: relative.join(name),
                before: storage::hash(&bytes),
                candidate: None,
                namespace: true,
                directory: false,
            });
        }
        names.sort();
        result.push(FilePlan {
            key: "namespace-directory".into(),
            kind: "namespace_directory".into(),
            name: "原插件独立目录（仅空目录）".into(),
            relative,
            before: digest(&names)?,
            candidate: None,
            namespace: true,
            directory: true,
        });
    }
    Ok((result, blockers))
}
fn ancestors(root: &Path, file: &FilePlan) -> AppResult<()> {
    storage::ordinary(root, true)?;
    let mut path = root.to_path_buf();
    let parent = file.relative.parent().ok_or_else(storage::error)?;
    for part in parent.components() {
        if !matches!(part, std::path::Component::Normal(_)) {
            return Err(storage::error());
        }
        path.push(part);
        storage::ordinary(&path, true)?;
    }
    if file.directory {
        storage::ordinary(&root.join(&file.relative), true)?;
    }
    Ok(())
}
pub(super) fn verify(root: &Path, files: &[FilePlan]) -> AppResult<()> {
    for f in files {
        ancestors(root, f)?;
        let full = root.join(&f.relative);
        if f.directory {
            storage::ordinary(&full, true)?;
            let mut names = Vec::new();
            for e in std::fs::read_dir(full).map_err(|_| storage::error())? {
                names.push(
                    e.map_err(|_| storage::error())?
                        .file_name()
                        .into_string()
                        .map_err(|_| storage::error())?,
                )
            }
            names.sort();
            if digest(&names)? != f.before {
                return Err(preflight());
            }
        } else {
            let bytes = storage::read(&full, 4 * 1024 * 1024)?.ok_or_else(preflight)?;
            if storage::hash(&bytes) != f.before {
                return Err(preflight());
            }
        }
    }
    Ok(())
}
pub(super) fn apply(root: &Path, file: &FilePlan) -> AppResult<Value> {
    ancestors(root, file)?;
    let path = root.join(&file.relative);
    if file.directory {
        storage::ordinary(&path, true)?;
        if std::fs::read_dir(&path)
            .map_err(|_| storage::error())?
            .next()
            .is_some()
        {
            return Err(preflight());
        }
        std::fs::remove_dir(&path).map_err(|_| storage::error())?;
    } else {
        let before = storage::read(&path, 4 * 1024 * 1024)?.ok_or_else(storage::error)?;
        if storage::hash(&before) != file.before {
            return Err(preflight());
        }
        if let Some(bytes) = &file.candidate {
            storage::write(&path, bytes)?;
        } else {
            storage::ordinary(&path, false)?;
            std::fs::remove_file(&path).map_err(|_| storage::error())?;
        }
    }
    #[cfg(unix)]
    std::fs::File::open(path.parent().ok_or_else(storage::error)?)
        .and_then(|f| f.sync_all())
        .map_err(|_| storage::error())?;
    Ok(
        json!({"beforeSha256":file.before,"afterSha256":file.candidate.as_ref().map(|b|storage::hash(b)),"ownedAction":"fixed-target-write"}),
    )
}
