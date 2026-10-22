//! Host-only Android installer boundary. Never exposed as plugin JavaScript commands.
use serde::{Deserialize, Serialize};

fn operation_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateRequest {
    pub action: String,
    pub operation_id: Option<String>,
    pub asset_url: Option<String>,
    pub asset_name: Option<String>,
    pub sha256: Option<String>,
    pub size_bytes: Option<String>,
    pub version: Option<String>,
}
impl AppUpdateRequest {
    pub fn action(action: &str, operation_id: Option<String>) -> Self {
        Self {
            action: action.into(),
            operation_id,
            asset_url: None,
            asset_name: None,
            sha256: None,
            size_bytes: None,
            version: None,
        }
    }
    pub fn valid(&self) -> bool {
        if !matches!(
            self.action.as_str(),
            "status" | "download" | "install" | "confirm" | "cancel" | "settings" | "store"
        ) {
            return false;
        }
        if self.action == "status" || self.action == "settings" || self.action == "store" {
            return self.operation_id.is_none()
                && self.asset_url.is_none()
                && self.asset_name.is_none()
                && self.sha256.is_none()
                && self.size_bytes.is_none()
                && self.version.is_none();
        }
        if !self.operation_id.as_ref().is_some_and(|v| operation_id(v)) {
            return false;
        }
        if self.action == "download" {
            self.sha256.as_ref().is_some_and(|s| {
                s.len() == 64
                    && s.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            }) && self
                .size_bytes
                .as_ref()
                .and_then(|s| s.parse::<u64>().ok())
                .is_some_and(|s| s > 0 && s <= 512 * 1024 * 1024)
                && self.asset_url.as_ref().is_some_and(|s| {
                    self.version.as_ref().zip(self.asset_name.as_ref()).is_some_and(|(version, name)| {
                        s == &format!("https://github.com/zerodenet/znet-sink/releases/download/{version}/{name}")
                            || s == &format!("https://github.com/zerodenet/znet-sink/releases/download/v{version}/{name}")
                    })
                })
                && self.asset_name.as_ref().is_some_and(|s| {
                    s.len() <= 180
                        && s.ends_with(".apk")
                        && s.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                })
                && self
                    .version
                    .as_ref()
                    .is_some_and(|s| !s.is_empty() && s.len() <= 256 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b".+-".contains(&b)))
        } else {
            self.asset_url.is_none()
                && self.asset_name.is_none()
                && self.sha256.is_none()
                && self.size_bytes.is_none()
                && self.version.is_none()
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppUpdateSnapshot {
    pub state: String,
    pub operation_id: Option<String>,
    pub reason: Option<String>,
    pub package_name: String,
    pub installed_version: String,
    pub installed_version_code: String,
    pub target_version: Option<String>,
    pub target_version_code: Option<String>,
    pub downloaded_bytes: String,
    pub size_bytes: Option<String>,
    pub asset_name: Option<String>,
    pub sha256: Option<String>,
    pub install_permission: String,
    pub confirmation_available: bool,
    pub installer_status: Option<i32>,
}
impl AppUpdateSnapshot {
    pub fn valid(&self) -> bool {
        matches!(
            self.state.as_str(),
            "idle"
                | "queued"
                | "downloading"
                | "paused"
                | "verifying"
                | "ready"
                | "preparing-install"
                | "awaiting-user"
                | "installing"
                | "installed-confirmed"
                | "cancelled"
                | "failed"
                | "unknown"
                | "store-managed"
        ) && matches!(
            self.install_permission.as_str(),
            "allowed" | "blocked" | "system-confirmation"
        ) && self.installed_version_code.parse::<u64>().is_ok()
            && self.downloaded_bytes.parse::<u64>().is_ok()
            && self.operation_id.as_ref().is_none_or(|v| operation_id(v))
            && self.target_version.as_ref().is_none_or(|v| v.len() <= 256)
            && self
                .target_version_code
                .as_ref()
                .is_none_or(|v| v.parse::<u64>().is_ok())
            && self.size_bytes.as_ref().is_none_or(|v| {
                v.parse::<u64>()
                    .is_ok_and(|n| n > 0 && n <= 512 * 1024 * 1024)
            })
            && self.sha256.as_ref().is_none_or(|v| {
                v.len() == 64
                    && v.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
            && self.asset_name.as_ref().is_none_or(|v| v.len() <= 180)
            && (!self.confirmation_available || self.state == "awaiting-user")
            && self.package_name.len() <= 256
            && self.installed_version.len() <= 256
            && self.reason.as_ref().is_none_or(|v| {
                v.len() <= 128
                    && v.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_boundary_rejects_paths_and_unscoped_mutations() {
        assert!(AppUpdateRequest::action("status", None).valid());
        assert!(!AppUpdateRequest::action("install", None).valid());
        assert!(!AppUpdateRequest::action("execute-file", None).valid());
        let mut r = AppUpdateRequest::action("status", None);
        r.asset_url = Some("file:///tmp/evil.apk".into());
        assert!(!r.valid());
        let mut download = AppUpdateRequest::action(
            "download",
            Some("e2695293-5580-4eec-9437-a1e6b2621111".into()),
        );
        download.asset_name = Some("client.apk".into());
        download.version = Some("1.0.0".into());
        download.sha256 = Some("a".repeat(64));
        download.size_bytes = Some("100".into());
        download.asset_url = Some(
            "https://github.com/zerodenet/znet-sink/releases/download/v1.0.0/client.apk".into(),
        );
        assert!(download.valid());
        download
            .asset_url
            .as_mut()
            .unwrap()
            .push_str("?token=untrusted");
        assert!(!download.valid());
        assert!(!operation_id("------------------------------------"));
    }
}
