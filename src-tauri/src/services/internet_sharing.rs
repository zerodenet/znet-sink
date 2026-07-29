use crate::errors::{AppError, AppResult};

/// Returns whether Windows Internet Connection Sharing is currently active.
/// `None` means the current platform does not expose Windows ICS.
pub fn is_active() -> AppResult<Option<bool>> {
    is_active_platform()
}

#[cfg(target_os = "windows")]
fn is_active_platform() -> AppResult<Option<bool>> {
    const SCRIPT: &str = r#"
$manager = New-Object -ComObject HNetCfg.HNetShare
$active = $false
foreach ($connection in $manager.EnumEveryConnection) {
  $sharing = $manager.INetSharingConfigurationForINetConnection.Invoke($connection)
  if ($sharing.SharingEnabled) {
    $active = $true
    break
  }
}
if ($active) { [Console]::Out.Write('active') } else { [Console]::Out.Write('inactive') }
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

    parse_status(&String::from_utf8_lossy(&output.stdout)).map(Some)
}

#[cfg(not(target_os = "windows"))]
fn is_active_platform() -> AppResult<Option<bool>> {
    Ok(None)
}

fn parse_status(output: &str) -> AppResult<bool> {
    match output.trim() {
        "active" => Ok(true),
        "inactive" => Ok(false),
        value => Err(AppError::internal(format!(
            "unexpected Windows Internet Connection Sharing status: {value}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_status;

    #[test]
    fn parses_internet_sharing_status() {
        assert!(parse_status("active\r\n").unwrap());
        assert!(!parse_status("inactive").unwrap());
        assert!(parse_status("unknown").is_err());
    }
}
