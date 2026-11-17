pub mod download;
use semver::Version;
use serde::Serialize;
use serde_json::Value;
use tauri::{Manager, ResourceId, Url, Webview};
use tauri_plugin_updater::UpdaterExt;

use crate::errors::{AppError, AppResult};

const RELEASE_DOWNLOAD_ROOT: &str = "https://github.com/zerodenet/znet-sink/releases/download";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateMetadata {
    rid: ResourceId,
    current_version: String,
    version: String,
    date: Option<String>,
    body: Option<String>,
    raw_json: Value,
}

#[tauri::command]
pub async fn app_check_release(
    webview: Webview,
    tag_name: String,
) -> AppResult<Option<AppUpdateMetadata>> {
    check_update(webview, Some(tag_name)).await
}

#[tauri::command]
pub async fn app_check_update(webview: Webview) -> AppResult<Option<AppUpdateMetadata>> {
    check_update(webview, None).await
}

async fn check_update(
    webview: Webview,
    tag_name: Option<String>,
) -> AppResult<Option<AppUpdateMetadata>> {
    let mut builder = webview
        .updater_builder()
        .timeout(std::time::Duration::from_secs(30));
    let expected = if let Some(tag) = &tag_name {
        builder = builder
            .endpoints(vec![release_manifest_endpoint(tag)?])
            .map_err(updater_error)?
            .version_comparator(|current, release| release.version != current);
        Some(release_version(tag)?)
    } else {
        None
    };
    use znet_client_capabilities::network::{default_route, Route};
    builder = match default_route() {
        Route::Direct | Route::Default => builder.no_proxy(),
        Route::System => builder,
        Route::Proxy(proxy) => builder
            .proxy(Url::parse(&proxy).map_err(|_| AppError::invalid_argument("更新代理地址无效"))?),
    };
    let updater = builder.build().map_err(updater_error)?;

    let Some(update) = updater.check().await.map_err(updater_error)? else {
        return Ok(None);
    };
    if let Some(expected_version) = expected.filter(|version| update.version != version.to_string())
    {
        return Err(AppError::conflict(
            "app_release",
            tag_name.unwrap_or_default(),
            format!(
                "selected release manifest announces v{} instead of v{expected_version}",
                update.version
            ),
        ));
    }

    let metadata = AppUpdateMetadata {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        date: update.date.map(|date| date.to_string()),
        body: update.body.clone(),
        raw_json: update.raw_json.clone(),
        rid: webview.resources_table().add(update),
    };
    Ok(Some(metadata))
}

fn release_manifest_endpoint(tag_name: &str) -> AppResult<Url> {
    let version = release_version(tag_name)?;
    let canonical_tag = format!("v{version}");

    Url::parse(&format!(
        "{RELEASE_DOWNLOAD_ROOT}/{canonical_tag}/latest.json"
    ))
    .map_err(|error| AppError::internal(format!("invalid release manifest URL: {error}")))
}

fn release_version(tag_name: &str) -> AppResult<Version> {
    let version_text = tag_name.strip_prefix('v').unwrap_or(tag_name);
    let version = Version::parse(version_text)
        .map_err(|_| AppError::invalid_argument("release tag must be a valid semantic version"))?;
    let canonical_tag = format!("v{version}");

    if tag_name != canonical_tag && tag_name != version.to_string() {
        return Err(AppError::invalid_argument(
            "release tag must use its canonical semantic-version form",
        ));
    }

    Ok(version)
}

fn updater_error(error: tauri_plugin_updater::Error) -> AppError {
    AppError::internal(format!("app updater failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::release_manifest_endpoint;

    #[test]
    fn builds_manifest_url_for_stable_and_prerelease_tags() {
        assert_eq!(
            release_manifest_endpoint("v0.0.16").unwrap().as_str(),
            "https://github.com/zerodenet/znet-sink/releases/download/v0.0.16/latest.json"
        );
        assert_eq!(
            release_manifest_endpoint("0.0.17-rc.2").unwrap().as_str(),
            "https://github.com/zerodenet/znet-sink/releases/download/v0.0.17-rc.2/latest.json"
        );
    }

    #[test]
    fn rejects_non_semver_and_noncanonical_release_tags() {
        assert!(release_manifest_endpoint("latest").is_err());
        assert!(release_manifest_endpoint("release-v0.0.16").is_err());
        assert!(release_manifest_endpoint("v0.0.016").is_err());
    }
}
