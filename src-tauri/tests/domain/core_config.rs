use gui_lib::models::app_config::AppCoreConfig;
use gui_lib::services::core_config::{inspect_from_config, snapshot_from_config, write_core_config};
use serde_json::json;

#[test]
fn default_zero_core_config_requires_explicit_executable() {
    let snapshot = snapshot_from_config(&AppCoreConfig::default()).unwrap();

    assert_eq!(snapshot.kernel, "zero");
    assert!(snapshot.executable_path.is_none());
    assert!(!snapshot.executable_exists);
    assert!(snapshot
        .warnings
        .iter()
        .any(|warning| warning.contains("core executable path is not configured")));

    #[cfg(windows)]
    {
        assert_eq!(snapshot.endpoint.transport, "named-pipe");
        assert_eq!(snapshot.endpoint.path, r"\\.\pipe\zero-control");
    }

    #[cfg(unix)]
    {
        assert_eq!(snapshot.endpoint.transport, "unix-socket");
        assert!(snapshot.endpoint.path.ends_with("zero-control.sock"));
        assert!(!snapshot.launch_args.contains(&"--control-socket".to_string()));
    }
}

#[test]
fn core_inspection_exposes_read_only_public_info() {
    let info = inspect_from_config(&AppCoreConfig::default()).unwrap();

    assert_eq!(info.kernel, "zero");
    assert!(!info.executable_exists);
    assert!(info.executable_path.is_none());
    assert!(info.download_url.is_some());
    assert!(info
        .warnings
        .iter()
        .any(|warning| warning.contains("core executable path is not configured")));
}

#[test]
fn explicit_socket_overrides_platform_default() {
    let config = AppCoreConfig {
        socket: Some(custom_socket()),
        ..AppCoreConfig::default()
    };

    let snapshot = snapshot_from_config(&config).unwrap();

    assert_eq!(snapshot.endpoint.path, custom_socket());
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
