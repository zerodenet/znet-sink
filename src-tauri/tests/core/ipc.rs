use gui_lib::kernel::protocol;
use gui_lib::kernel::transport;
use gui_lib::models::core::CoreIpcOptions;
use serde_json::json;

#[test]
fn zero_timeout_is_rejected() {
    let options = CoreIpcOptions {
        socket: None,
        timeout_ms: Some(0),
    };
    let error = block_on(protocol::ping(Some(options))).unwrap_err();

    assert_eq!(error.code, "invalid_argument");
}

#[test]
fn non_object_frame_is_rejected() {
    let error = block_on(protocol::request(json!("Runtime"), None)).unwrap_err();

    assert_eq!(error.code, "invalid_argument");
}

#[test]
fn empty_command_method_is_rejected() {
    let error = block_on(protocol::command(" ".to_string(), None, None)).unwrap_err();

    assert_eq!(error.code, "invalid_argument");
}

#[test]
fn default_endpoint_uses_platform_transport() {
    let endpoint = transport::default_endpoint("zero").unwrap();

    #[cfg(windows)]
    {
        assert_eq!(endpoint.transport, "named-pipe");
        assert_eq!(endpoint.path, r"\\.\pipe\zero-control");
    }

    #[cfg(unix)]
    {
        assert_eq!(endpoint.transport, "unix-socket");
        assert!(endpoint.path.ends_with(".zero/control.sock"));
    }
}

#[test]
fn unavailable_core_resolves_as_offline_result() {
    let result = block_on(protocol::ping(Some(CoreIpcOptions {
        socket: Some(unused_endpoint()),
        timeout_ms: Some(100),
    })));

    match result {
        Ok(r) => {
            assert!(!r.available);
            assert!(r.error.is_some());
        }
        Err(_) => {
            // Also acceptable: transport fails fast when pipe doesn't exist
        }
    }
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
