use crate::errors::{AppError, AppResult};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::File,
    io::Read,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use znet_client_core::capability::{Budget, Lease, Manager, Permission};
use znet_plugin_sandbox::distribution::{directory::Directory, remote::Remote, store::Store};

pub(super) fn failure(error: impl std::fmt::Display) -> AppError {
    AppError::invalid_argument(format!("插件操作失败：{}", safe_reason(error)))
}
pub(super) fn marketplace_failure(error: impl std::fmt::Display) -> AppError {
    AppError::invalid_argument(format!("无法读取插件中心元数据：{}", safe_reason(error)))
}
pub(super) fn package_failure(error: impl std::fmt::Display) -> AppError {
    let reason = safe_reason(error);
    if reason.starts_with("从作者 GitHub Release 下载或校验插件失败：") {
        return AppError::invalid_argument(reason);
    }
    AppError::invalid_argument(format!(
        "从作者 GitHub Release 下载或校验插件失败：{}",
        reason
    ))
}
fn safe_reason(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    if message.contains("已保留进度")
        || message.contains("大小")
        || message.contains("校验")
        || message.contains("缓存")
        || message.contains("磁盘")
    {
        return message.chars().take(240).collect();
    }
    if message.contains("signature") {
        return "签名与插件中心登记不一致".into();
    }
    if message.contains("digest") {
        return "SHA256 与插件中心登记不一致".into();
    }
    if message.contains("identity") || message.contains("publisher") {
        return "插件身份或发布者与插件中心登记不一致".into();
    }
    if message.contains("compatible") || message.contains("host version") {
        return "该版本与当前客户端不兼容".into();
    }
    if message.contains("limit") || message.contains("size") {
        return "安装包大小与插件中心登记不一致".into();
    }
    "网络请求失败或元数据格式无效".into()
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
            "https://zerodenet.github.io",
        )]),
    )?;
    Remote::with_fetch(|url, limit| {
        let response =
            znet_client_capabilities::network::get(&lease, url, "ZNet-Sink-Plugin/1", limit, &[])
                .and_then(|resource| resource.take(&lease))?;
        if response.status != 200 {
            return Err("directory response failed".into());
        }
        Ok(response.body)
    })
    .and_then(|remote| remote.for_host(env!("CARGO_PKG_VERSION")))
    .and_then(|remote| remote.directory())
    .map_err(marketplace_failure)
}

pub(super) fn download_package(
    url: &str,
    limit: usize,
    expected_sha256: &str,
    progress: impl Fn(crate::services::download::Progress),
) -> AppResult<Vec<u8>> {
    let source = reqwest::Url::parse(url)
        .map_err(|_| AppError::invalid_argument("插件中心返回了无效的安装包地址"))?;
    if source.scheme() != "https"
        || source.host_str() != Some("github.com")
        || !source.username().is_empty()
        || source.password().is_some()
        || source.port().is_some()
        || source.query().is_some()
        || source.fragment().is_some()
    {
        return Err(AppError::invalid_argument(
            "插件安装包必须来自作者仓库的 GitHub Release",
        ));
    }
    let client = reqwest::blocking::Client::builder()
        .user_agent("ZNet-Sink-Plugin/1")
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(600))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let url = attempt.url();
            let host = url.host_str().unwrap_or("");
            let trusted = url.scheme() == "https"
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none()
                && (host == "github.com" || host.ends_with(".githubusercontent.com"));
            if attempt.previous().len() >= 5 || !trusted {
                attempt.error("untrusted GitHub release redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(package_failure)?;
    let identity = format!("plugin:{expected_sha256}");
    let download =
        crate::services::download::fetch_bounded(&client, url, &identity, limit as u64, progress)
            .map_err(|error| package_failure(error.message))?;
    let file = File::open(&download.path).map_err(package_failure)?;
    let mut bytes = Vec::with_capacity(limit.min(1024 * 1024));
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(package_failure)?;
    if bytes.len() > limit {
        download
            .discard()
            .map_err(|error| package_failure(error.message))?;
        return Err(AppError::invalid_argument("插件安装包超过允许的大小限制"));
    }
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual != expected_sha256 {
        download
            .discard()
            .map_err(|error| package_failure(error.message))?;
        return Err(AppError::invalid_argument(
            "插件安装包 SHA256 校验失败，已清除损坏缓存",
        ));
    }
    Ok(bytes)
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
