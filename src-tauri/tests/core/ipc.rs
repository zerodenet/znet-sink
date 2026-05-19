use gui_lib::models::core::CoreIpcOptions;
use gui_lib::services::control_plane::{command, default_endpoint, ping, request};
use serde_json::json;

#[test]
fn zero_timeout_is_rejected() {
    let options = CoreIpcOptions {
        socket: None,
        timeout_ms: Some(0),
    };
    let error = block_on(ping(Some(options))).unwrap_err();

    assert_eq!(error.code, "invalid_argument");
}

#[test]
fn non_object_frame_is_rejected() {
    let error = block_on(request(json!("Runtime"), None)).unwrap_err();

    assert_eq!(error.code, "invalid_argument");
}

#[test]
fn empty_command_method_is_rejected() {
    let error = block_on(command(" ".to_string(), None, None)).unwrap_err();

    assert_eq!(error.code, "invalid_argument");
}

#[test]
fn default_endpoint_uses_platform_transport() {
    let endpoint = default_endpoint().unwrap();

    #[cfg(windows)]
    {
        assert_eq!(endpoint.transport, "named-pipe");
        assert_eq!(endpoint.path, r"\\.\pipe\zero-control");
    }

    #[cfg(unix)]
    {
        assert_eq!(endpoint.transport, "unix-socket");
        assert!(endpoint.path.ends_with("zero-control.sock"));
    }
}

#[test]
fn unavailable_core_resolves_as_offline_result() {
    let result = block_on(ping(Some(CoreIpcOptions {
        socket: Some(unused_endpoint()),
        timeout_ms: Some(100),
    })))
    .unwrap();

    assert!(!result.available);
    assert!(result.error.is_some());
}

fn block_on<T>(future: impl std::future::Future<Output = T> + Send + 'static) -> T
where
    T: Send + 'static,
{
    tauri::async_runtime::block_on(future)
}

fn unused_endpoint() -> String {
    #[cfg(windows)]
    {
        r"\\.\pipe\znet-sink-test-missing-zero-control".to_string()
    }

    #[cfg(unix)]
    {
        "/tmp/znet-sink-test-missing-zero-control.sock".to_string()
    }
}
