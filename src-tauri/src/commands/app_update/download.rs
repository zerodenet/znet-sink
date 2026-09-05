use crate::errors::{AppError, AppResult};
use crate::services::download::{self, Progress};
use base64::Engine;
use minisign_verify::{PublicKey, Signature};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{ipc::Channel, Manager, ResourceId, Webview};
use tauri_plugin_updater::Update;

static BUSY: AtomicBool = AtomicBool::new(false);
struct Operation;
impl Operation {
    fn begin() -> AppResult<Self> {
        BUSY.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| AppError::internal("应用更新正在进行，请等待当前操作完成"))?;
        Ok(Self)
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}

#[tauri::command]
pub async fn app_download_update(
    webview: Webview,
    rid: ResourceId,
    on_event: Channel<Progress>,
) -> AppResult<()> {
    let operation = Operation::begin()?;
    let update = webview
        .resources_table()
        .get::<Update>(rid)
        .map_err(|e| AppError::internal(e.to_string()))?;
    let pubkey = public_key(&webview)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _operation = operation;
        let mut builder = reqwest::blocking::Client::builder()
            .user_agent("znet-sink-updater")
            .connect_timeout(Duration::from_secs(15))
            .default_headers(update.headers.clone());
        if update.no_proxy {
            builder = builder.no_proxy();
        } else if let Some(proxy) = &update.proxy {
            builder = builder.proxy(
                reqwest::Proxy::all(proxy.as_str())
                    .map_err(|e| AppError::internal(e.to_string()))?,
            );
        }
        let client = builder
            .build()
            .map_err(|e| AppError::internal(e.to_string()))?;
        let artifact = download::fetch(
            &client,
            update.download_url.as_str(),
            &identity(&update),
            |event| {
                let _ = on_event.send(event);
            },
        )?;
        verify_artifact(&artifact, &update.signature, &pubkey)?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
}

#[tauri::command]
pub async fn app_install_update(webview: Webview, rid: ResourceId) -> AppResult<()> {
    let operation = Operation::begin()?;
    let update = webview
        .resources_table()
        .get::<Update>(rid)
        .map_err(|e| AppError::internal(e.to_string()))?;
    let pubkey = public_key(&webview)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _operation = operation;
        let artifact = download::cached(update.download_url.as_str(), &identity(&update))?;
        // Tauri Update::install does not verify signatures itself. Verify the
        // exact bytes passed to it, including cached downloads after a restart.
        let bytes = verify_artifact(&artifact, &update.signature, &pubkey)?;
        update
            .install(bytes)
            .map_err(|e| AppError::internal(format!("更新安装失败：{e}")))
    })
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
}

fn identity(update: &Update) -> String {
    format!(
        "app:{}:{}:{}",
        update.version, update.target, update.signature
    )
}
fn public_key(webview: &Webview) -> AppResult<String> {
    webview
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|p| p.get("pubkey"))
        .and_then(|p| p.as_str())
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| AppError::internal("未配置更新签名公钥，拒绝安装"))
}
fn verify_artifact(
    artifact: &download::Download,
    signature: &str,
    pubkey: &str,
) -> AppResult<Vec<u8>> {
    let bytes = std::fs::read(&artifact.path).map_err(|e| AppError::internal(e.to_string()))?;
    if let Err(error) = verify(&bytes, signature, pubkey) {
        artifact.discard()?;
        return Err(error);
    }
    Ok(bytes)
}
fn verify(bytes: &[u8], signature: &str, pubkey: &str) -> AppResult<()> {
    let decode = |value: &str| -> AppResult<String> {
        let data = base64::engine::general_purpose::STANDARD
            .decode(value)
            .map_err(|_| AppError::internal("更新签名编码无效"))?;
        String::from_utf8(data).map_err(|_| AppError::internal("更新签名格式无效"))
    };
    let key =
        PublicKey::decode(&decode(pubkey)?).map_err(|_| AppError::internal("更新签名公钥无效"))?;
    let signature =
        Signature::decode(&decode(signature)?).map_err(|_| AppError::internal("更新签名无效"))?;
    key.verify(bytes, &signature, true)
        .map_err(|_| AppError::internal("更新包签名校验失败，已清除损坏缓存，请重新下载"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_bytes_verify_but_tampered_cached_bytes_do_not() {
        // Public test vector from minisign-verify (MIT licensed).
        let key = "untrusted comment: minisign public key E7620F1842B4E81F\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
        let signature = "untrusted comment: signature from minisign secret key\nRWQf6LRCGA9i59SLOFxz6NxvASXDJeRtuZykwQepbDEGt87ig1BNpWaVWuNrm73YiIiJbq71Wi+dP9eKL8OC351vwIasSSbXxwA=\ntrusted comment: timestamp:1555779966\tfile:test\nQtKMXWyYcwdpZAlPF7tE2ENJkRd1ujvKjlj1m9RtHTBnZPa5WKU5uWRs5GoP5M/VqE81QFuMKI5k/SfNQUaOAA==";
        let key = base64::engine::general_purpose::STANDARD.encode(key);
        let signature = base64::engine::general_purpose::STANDARD.encode(signature);
        assert!(verify(b"test", &signature, &key).is_ok());
        assert!(verify(b"Test", &signature, &key).is_err());
    }
    #[test]
    fn invalid_signatures_never_accept_cached_bytes() {
        assert!(verify(b"downloaded executable", "invalid", "invalid").is_err());
    }
    #[test]
    fn download_and_install_exclude_each_other_and_release_after_failure() {
        let guard = Operation::begin().unwrap();
        assert!(Operation::begin().is_err());
        drop(guard);
        assert!(Operation::begin().is_ok());
    }
}
