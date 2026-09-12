use crate::errors::{AppError, AppResult};
use std::{
    collections::BTreeSet,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use znet_client_core::capability::{Budget, Lease, Manager, Permission};
use znet_plugin_sandbox::distribution::{
    directory::Directory, remote::DIRECTORY_URL, store::Store,
};

pub(super) fn failure(_: impl std::fmt::Display) -> AppError {
    AppError::invalid_argument("插件校验或操作失败，请检查安装包、中央登记及网络连接")
}
pub(super) fn lease(manager: &Manager, permissions: BTreeSet<Permission>) -> AppResult<Lease> {
    let policy = manager
        .admit(
            "builtin.plugins".into(),
            permissions.clone(),
            permissions.clone(),
            &permissions,
        )
        .map_err(failure)?;
    policy
        .authorize(permissions, Duration::from_secs(30))
        .map_err(failure)?;
    policy
        .begin(
            Budget {
                calls: 4,
                resource_bytes: 5 * 1024 * 1024,
                timeout: Duration::from_secs(30),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .map_err(failure)
}
pub(super) fn directory(manager: &Manager) -> AppResult<Directory> {
    let lease = lease(
        manager,
        BTreeSet::from([Permission::new(
            "network.get",
            "https://raw.githubusercontent.com",
        )]),
    )?;
    let response = znet_client_capabilities::network::get(
        &lease,
        DIRECTORY_URL,
        "ZNet-Sink-Plugin/1",
        1024 * 1024,
        &[],
    )
    .and_then(|resource| resource.take(&lease))
    .map_err(failure)?;
    if response.status != 200 {
        return Err(failure("directory response"));
    }
    Directory::parse(&response.body).map_err(failure)
}
pub(super) fn store() -> AppResult<Store> {
    Store::new(super::super::data_dir()?.join("plugins")).map_err(failure)
}
pub(super) fn manage<T>(manager: &Manager, work: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
    let permission = Permission::new("plugin.manage", "local");
    let lease = lease(manager, BTreeSet::from([permission.clone()]))?;
    let mut failure = None;
    let result = lease
        .execute(&permission, || {
            work().map(|value| (value, 0)).map_err(|error| {
                failure = Some(error);
                znet_client_core::capability::Error::Transport
            })
        })
        .and_then(|value| value.take(&lease));
    result.map_err(|error| failure.unwrap_or_else(|| self::failure(error)))
}
