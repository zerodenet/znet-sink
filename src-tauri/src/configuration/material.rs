//! Ordinary import boundary. File reads and material preparation share one lease.
//! Protected sources must not publish into the serializable ordinary profile model.
use crate::{
    errors::{AppError, AppResult},
    services::proxy_config::parse_config_content,
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use znet_client_capabilities::files;
use znet_client_core::capability::{Budget, Manager, Permission};
const MAX_BYTES: usize = 16 * 1024 * 1024;

pub(crate) fn import(
    manager: &Manager,
    content: Option<String>,
    path: Option<String>,
) -> AppResult<Value> {
    let selected = if content.is_none() {
        path.map(|path| files::Selection::from_user_path(path.into()))
    } else {
        None
    };
    let prepare = Permission::new("configuration.prepare", "import");
    let mut grants = BTreeSet::from([prepare.clone()]);
    if let Some(selected) = &selected {
        grants.insert(selected.permission());
    }
    let failure = |error| AppError::invalid_argument(format!("配置材料读取或准备失败：{error}"));
    let policy = manager
        .admit(
            "builtin.config-import".into(),
            grants.clone(),
            grants.clone(),
            &grants,
        )
        .map_err(failure)?;
    policy
        .authorize(grants, Duration::from_secs(30))
        .map_err(failure)?;
    let lease = policy
        .begin(
            Budget {
                calls: 2,
                resource_bytes: MAX_BYTES,
                timeout: Duration::from_secs(30),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .map_err(failure)?;
    let content = match (content, selected) {
        (Some(text), _) => text,
        (_, Some(selected)) => {
            let bytes = files::read(&lease, &selected, MAX_BYTES)
                .and_then(|resource| resource.take(&lease))
                .map_err(failure)?;
            String::from_utf8(bytes)
                .map_err(|_| AppError::invalid_argument("配置文件必须使用 UTF-8 编码"))?
        }
        _ => {
            return Err(AppError::invalid_argument(
                "content or path is required to import proxy config",
            ))
        }
    };
    // Preserve detailed parser errors without putting source text in operation metadata.
    let mut parse_error = None;
    lease
        .execute(&prepare, || {
            if content.len() > MAX_BYTES {
                return Err(znet_client_core::capability::Error::BudgetExceeded);
            }
            let value = parse_config_content(&content).map_err(|error| {
                parse_error = Some(error);
                znet_client_core::capability::Error::InvalidRequest
            })?;
            let bytes = serde_json::to_vec(&value)
                .map_err(|_| znet_client_core::capability::Error::InvalidRequest)?
                .len();
            Ok((value, bytes))
        })
        .and_then(|resource| resource.take(&lease))
        .map_err(|error| parse_error.unwrap_or_else(|| failure(error)))
}

#[cfg(test)]
#[path = "material_tests.rs"]
mod tests;
