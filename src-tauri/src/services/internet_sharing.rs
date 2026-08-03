use crate::errors::{AppError, AppResult};
use serde::Deserialize;

/// Returns whether Windows Internet Connection Sharing is currently active.
/// `None` means the current platform does not expose Windows ICS.
pub fn is_active() -> AppResult<Option<bool>> {
    is_active_platform()
}

#[cfg(target_os = "windows")]
fn is_active_platform() -> AppResult<Option<bool>> {
    const SCRIPT: &str = r#"
$manager = New-Object -ComObject HNetCfg.HNetShare
$shared = @()
foreach ($connection in $manager.EnumEveryConnection) {
  $properties = $manager.NetConnectionProps.Invoke($connection)
  $sharing = $manager.INetSharingConfigurationForINetConnection.Invoke($connection)
  if ($sharing.SharingEnabled) {
    $shared += [PSCustomObject]@{
      name = [string]$properties.Name
      deviceName = [string]$properties.DeviceName
      status = [int]$properties.Status
      sharingType = [int]$sharing.SharingConnectionType
    }
  }
}
if ($shared.Count -eq 0) {
  [Console]::Out.Write('[]')
} else {
  [Console]::Out.Write((ConvertTo-Json -InputObject @($shared) -Compress))
}
"#;

    let output = super::common::background_command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .output()
        .map_err(|error| {
            AppError::internal(format!(
                "failed to inspect Windows Internet Connection Sharing: {error}"
            ))
        })?;

    if !output.status.success() {
        return Err(AppError::internal(format!(
            "failed to inspect Windows Internet Connection Sharing: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    parse_connections(&String::from_utf8_lossy(&output.stdout)).map(Some)
}

#[cfg(not(target_os = "windows"))]
fn is_active_platform() -> AppResult<Option<bool>> {
    Ok(None)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedConnection {
    name: String,
    device_name: String,
    status: i32,
    sharing_type: i32,
}

fn parse_connections(output: &str) -> AppResult<bool> {
    let connections: Vec<SharedConnection> =
        serde_json::from_str(output.trim()).map_err(|error| {
            AppError::internal(format!(
                "unexpected Windows Internet Connection Sharing status: {error}"
            ))
        })?;

    Ok(has_active_sharing_pair(&connections))
}

fn has_active_sharing_pair(connections: &[SharedConnection]) -> bool {
    // NETCON_STATUS_CONNECTED = 2; ICSSHARINGTYPE_PUBLIC = 0;
    // ICSSHARINGTYPE_PRIVATE = 1. A real ICS/hotspot topology needs both
    // sides. Requiring the pair avoids transient warnings while Windows is
    // reconciling adapters and only one side has been marked as shared.
    const CONNECTED: i32 = 2;
    const PUBLIC: i32 = 0;
    const PRIVATE: i32 = 1;

    let has_public = connections
        .iter()
        .any(|connection| connection.sharing_type == PUBLIC && connection.status == CONNECTED);
    let has_real_private = connections.iter().any(|connection| {
        connection.sharing_type == PRIVATE
            && connection.status == CONNECTED
            && !is_infrastructure_virtual_adapter(connection)
    });

    has_public && has_real_private
}

fn is_infrastructure_virtual_adapter(connection: &SharedConnection) -> bool {
    let identity = format!("{} {}", connection.name, connection.device_name).to_ascii_lowercase();
    [
        "hyper-v",
        "default switch",
        "vethernet",
        "windows subsystem for linux",
        "wsl",
        "docker",
        "vmware",
        "virtualbox",
    ]
    .iter()
    .any(|marker| identity.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::parse_connections;

    #[test]
    fn inactive_when_no_shared_connections_exist() {
        assert!(!parse_connections("[]").unwrap());
    }

    #[test]
    fn active_for_connected_public_and_physical_private_pair() {
        let output = r#"[
            {"name":"Ethernet","deviceName":"Intel Ethernet","status":2,"sharingType":0},
            {"name":"Local Area Connection","deviceName":"Microsoft Wi-Fi Direct Virtual Adapter","status":2,"sharingType":1}
        ]"#;

        assert!(parse_connections(output).unwrap());
    }

    #[test]
    fn inactive_for_transient_single_sided_sharing() {
        let output = r#"[
            {"name":"Ethernet","deviceName":"Intel Ethernet","status":2,"sharingType":0}
        ]"#;

        assert!(!parse_connections(output).unwrap());
    }

    #[test]
    fn ignores_hyper_v_private_switches() {
        let output = r#"[
            {"name":"Ethernet","deviceName":"Intel Ethernet","status":2,"sharingType":0},
            {"name":"vEthernet (Default Switch)","deviceName":"Hyper-V Virtual Ethernet Adapter","status":2,"sharingType":1}
        ]"#;

        assert!(!parse_connections(output).unwrap());
    }

    #[test]
    fn inactive_when_shared_pair_is_disconnected() {
        let output = r#"[
            {"name":"Ethernet","deviceName":"Intel Ethernet","status":2,"sharingType":0},
            {"name":"Local Area Connection","deviceName":"Microsoft Wi-Fi Direct Virtual Adapter","status":7,"sharingType":1}
        ]"#;

        assert!(!parse_connections(output).unwrap());
    }

    #[test]
    fn rejects_unexpected_ics_output() {
        assert!(parse_connections("unknown").is_err());
    }
}
