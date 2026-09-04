use gui_lib::models::app_config::AppCoreConfig;
use gui_lib::services::core_config::{
    inspect_from_config, snapshot_from_config, write_core_config,
};
use serde_json::json;

#[test]
fn explicitly_missing_zero_core_executable_is_reported() {
    let missing = missing_executable();
    let config = AppCoreConfig {
        executable_path: Some(missing.clone()),
        ..AppCoreConfig::default()
    };
    let snapshot = snapshot_from_config(&config).unwrap();

    assert_eq!(snapshot.kernel, "zero");
    assert_eq!(snapshot.executable_path.as_deref(), Some(missing.as_str()));
    assert!(!snapshot.executable_exists);
    assert!(snapshot
        .warnings
        .iter()
        .any(|warning| warning.contains("core executable does not exist")));

    #[cfg(windows)]
    {
        assert_eq!(snapshot.endpoint.transport, "named-pipe");
        assert_eq!(
            snapshot.endpoint.path,
            format!(r"\\.\pipe\zero-control-{}", std::process::id())
        );
    }

    #[cfg(unix)]
    {
        assert_eq!(snapshot.endpoint.transport, "unix-socket");
        assert!(snapshot
            .endpoint
            .path
            .ends_with(&format!("zero-control-{}.sock", std::process::id())));
        assert!(snapshot
            .launch_args
            .contains(&"--control-socket".to_string()));
    }
    assert!(snapshot
        .launch_args
        .contains(&"--parent-lifetime-stdin".to_string()));
}

#[test]
fn managed_unix_core_uses_socket_next_to_executable() {
    #[cfg(unix)]
    {
        let config = AppCoreConfig {
            executable_path: Some("/opt/znet/core/zero".to_string()),
            ..AppCoreConfig::default()
        };

        let snapshot = snapshot_from_config(&config).unwrap();

        assert_eq!(snapshot.endpoint.transport, "unix-socket");
        let expected = format!("/opt/znet/core/zero-control-{}.sock", std::process::id());
        assert_eq!(snapshot.endpoint.path, expected);
        assert!(snapshot
            .launch_args
            .windows(2)
            .any(|args| args == ["--control-socket", expected.as_str()]));
        assert!(snapshot
            .launch_args
            .contains(&"--parent-lifetime-stdin".to_string()));
    }
}

#[test]
fn core_inspection_exposes_read_only_public_info() {
    let missing = missing_executable();
    let config = AppCoreConfig {
        executable_path: Some(missing.clone()),
        ..AppCoreConfig::default()
    };
    let info = inspect_from_config(&config, false).unwrap();

    assert_eq!(info.kernel, "zero");
    assert!(!info.executable_exists);
    assert_eq!(info.executable_path.as_deref(), Some(missing.as_str()));
    assert!(info.recommended_install_dir.is_some());
    assert!(info
        .warnings
        .iter()
        .any(|warning| warning.contains("core executable does not exist")));
}

#[test]
fn explicit_socket_overrides_platform_default() {
    let config = AppCoreConfig {
        socket: Some(custom_socket()),
        ..AppCoreConfig::default()
    };

    let snapshot = snapshot_from_config(&config).unwrap();

    assert_eq!(snapshot.endpoint.path, custom_socket());
    assert!(snapshot
        .launch_args
        .contains(&"--parent-lifetime-stdin".to_string()));
}

#[test]
fn core_config_writer_persists_json_object() {
    let dir = std::env::temp_dir().join(format!("core-config-writer-{}", std::process::id()));
    let path = dir.join("zero-config.json");
    let content = json!({
        "outbounds": [{ "tag": "direct", "protocol": { "type": "direct" } }],
        "route": { "mode": { "type": "global", "outbound": "direct" } }
    });

    write_core_config(&path, &content).unwrap();
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();

    assert_eq!(saved, content);

    let _ = std::fs::remove_dir_all(dir);
}

fn custom_socket() -> String {
    #[cfg(windows)]
    {
        r"\\.\pipe\custom-zero-control".to_string()
    }

    #[cfg(unix)]
    {
        "/tmp/custom-zero-control.sock".to_string()
    }
}

fn missing_executable() -> String {
    std::env::temp_dir()
        .join("znet-sink-definitely-missing-zero")
        .to_string_lossy()
        .to_string()
}
