use gui_lib::models::app_config::AppCoreConfig;
use gui_lib::services::core_config::snapshot_from_config;

#[test]
fn missing_core_executable_is_not_launchable() {
    let snapshot = snapshot_from_config(&AppCoreConfig {
        executable_path: Some(missing_executable()),
        auto_start: true,
        ..AppCoreConfig::default()
    })
    .unwrap();

    let error = snapshot.validate_launchable().unwrap_err();

    assert!(error.contains("core executable does not exist"));
}

fn missing_executable() -> String {
    #[cfg(windows)]
    {
        r"C:\tmp\znet-sink-missing-zero.exe".to_string()
    }

    #[cfg(unix)]
    {
        "/tmp/znet-sink-missing-zero".to_string()
    }
}
