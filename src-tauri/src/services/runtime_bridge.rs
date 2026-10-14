//! Actual pinned IPC inspection for the shared bridge. Native ownership stays
//! unsupported until a real Proxy service/provider can register its adapter.
use crate::{
    errors::{AppError, AppResult},
    kernel::configuration::BoundControl,
    services::common::lock,
    state::app_state::AppState,
};
use serde::Serialize;
use std::time::Duration;
use znet_engine_client::bridge::{
    BridgeError, BridgeRequest, HostCaller, OperationReceipt, RuntimeBinding, HOST_IDENTITY,
    MAX_RESULT_BYTES,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub status: &'static str,
    pub transport: &'static str,
    pub core_instance_id: String,
    pub config_revision: String,
    pub receipt: OperationReceipt,
    pub vpn_observation: &'static str,
}

fn failure(reason: &'static str) -> AppError {
    AppError {
        code: "unsupported",
        message: reason.into(),
        details: Some(
            serde_json::json!({"status":"unsupported","reasonCode":"runtime_bridge_owner_not_available","dependencyIds":["CLIENT-01"]}),
        ),
    }
}

pub(crate) async fn inspect(
    state: &AppState,
    verified_window_label: &str,
) -> AppResult<Inspection> {
    let caller = HostCaller::from_window_label(verified_window_label).map_err(|_| AppError {
        code: "forbidden",
        message: "控制通道核对仅允许宿主主窗口".into(),
        details: None,
    })?;
    if crate::mobile_host::snapshot().mobile {
        return Err(crate::mobile_host::unavailable(
            "gui_runtime_bridge_inspect",
        ));
    }
    let scope = state.client_core_snapshot().scope;
    let profile_id = scope
        .profile_id
        .as_ref()
        .map(|id| id.0.clone())
        .ok_or_else(|| failure("尚未绑定当前配置，控制通道暂不支持核对。"))?;
    if scope.core_instance_id.0 == 0 {
        return Err(failure("尚未取得运行实例，控制通道暂不支持核对。"));
    }
    let core = lock(state.app_config(), "app_config")?.core.clone();
    if core.kernel != "zero" {
        return Err(failure("当前内核没有此控制通道适配器。"));
    }
    let discovery = znet_client_capabilities::host::operation(
        state.capabilities(),
        "builtin.runtime-bridge-discovery",
        znet_client_core::capability::Permission::new("runtime.inspect", "current-profile"),
        1,
        MAX_RESULT_BYTES,
        Duration::from_secs(5),
    )
    .map_err(|_| failure("控制通道读取能力或预算不可用。"))?;
    let options = super::core_config::ipc_options_from_app_config(&core);
    let resource = discovery
        .execute_async(
            &znet_client_core::capability::Permission::new("runtime.inspect", "current-profile"),
            async {
                tokio::time::timeout(Duration::from_secs(5), async {
                    let control = BoundControl::connect(options)
                        .await
                        .map_err(|_| znet_client_core::capability::Error::Transport)?;
                    let identity = control
                        .identity()
                        .await
                        .map_err(|_| znet_client_core::capability::Error::Transport)?;
                    Ok(((control, identity), 0))
                })
                .await
                .map_err(|_| znet_client_core::capability::Error::Transport)?
            },
        )
        .await
        .and_then(|resource| resource.take(&discovery))
        .map_err(|_| AppError::internal("控制通道运行身份读取未确认，请重新核对"))?;
    state.ensure_client_scope(Some(&scope))?;
    let (control, identity) = resource;
    let binding = RuntimeBinding {
        runtime: identity.clone(),
        profile_id,
        client_generation: scope.core_instance_id.0,
    };
    state.bind_runtime_bridge(&scope, binding.clone())?;
    let ticket = state
        .runtime_bridge()
        .issue(&caller)
        .map_err(|_| failure("控制通道未绑定或核对预算已满。"))?;
    let lease = znet_client_capabilities::host::operation(
        state.capabilities(),
        HOST_IDENTITY,
        znet_client_core::capability::Permission::new("runtime.query", binding.permission_scope()),
        1,
        MAX_RESULT_BYTES,
        Duration::from_secs(5),
    )
    .map_err(|_| failure("控制通道读取能力或预算不可用。"))?;
    // Local Read is a limit, never evidence of the IPC server's authorization.
    // Each actual query still passes the server's permission gate.
    let auth = zero_api::AuthContext {
        subject: None,
        permissions: vec![zero_api::Permission::Read],
    };
    let request = zero_api::QueryRequest::Runtime(Default::default());
    let result = state
        .runtime_bridge()
        .call(
            &control,
            ticket,
            BridgeRequest::Query(request),
            &lease,
            &auth,
        )
        .await
        .map_err(|error| match error {
            BridgeError::StaleBinding => AppError::conflict(
                "runtime_bridge",
                "",
                "实例、配置或通道代次已变化，请重新核对",
            ),
            BridgeError::Forbidden => AppError {
                code: "forbidden",
                message: "控制通道读取未获授权".into(),
                details: None,
            },
            BridgeError::Unavailable | BridgeError::Unsupported => {
                failure("当前控制通道能力暂不支持。")
            }
            _ => AppError::internal("控制通道读取结果未确认，请重新核对；不会重放写操作"),
        })?;
    state.ensure_client_scope(Some(&scope))?;
    if lock(state.app_config(), "app_config")?.core != core {
        state.runtime_bridge().invalidate();
        return Err(AppError::conflict(
            "runtime_bridge",
            "",
            "控制端点已变化，请重新核对",
        ));
    }
    // Do not expose the raw runtime reply, paths or error text to the WebView.
    Ok(confirmed_inspection(identity, result.receipt))
}

fn confirmed_inspection(
    identity: znet_engine_client::configuration::RuntimeIdentity,
    receipt: OperationReceipt,
) -> Inspection {
    Inspection {
        status: "confirmed",
        transport: "pinned-ipc",
        core_instance_id: identity.core_instance_id,
        config_revision: identity.config_revision.to_string(),
        receipt,
        vpn_observation: "unprovided",
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
        thread,
    };
    use znet_engine_client::bridge::{ReceiptState, RuntimeBridge};

    #[tokio::test]
    async fn actual_pinned_transport_rechecks_identity_and_exports_no_runtime_payload() {
        for replace_instance in [false, true] {
            let directory = tempfile::tempdir_in("/tmp").unwrap();
            let path = directory.path().join("bridge.sock");
            let listener = UnixListener::bind(&path).unwrap();
            let worker = thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut writer = stream.try_clone().unwrap();
                let mut queries = 0;
                for line in BufReader::new(stream).lines() {
                    let Ok(line) = line else { break };
                    let request: serde_json::Value = serde_json::from_str(&line).unwrap();
                    let result = if request["type"] == "subscribe" {
                        json!("subscribed")
                    } else {
                        assert_eq!(request["type"], "query");
                        assert!(request["request"].get("runtime").is_some());
                        queries += 1;
                        json!({"runtime":{"core_instance_id":if replace_instance&&queries==4 {"new-instance"}else{"actual-instance"},"config_revision":9007199254740993u64,"secret":"never-export-runtime-secret"}})
                    };
                    if writeln!(
                        writer,
                        "{}",
                        json!({"api_id":"zero.api.v1","id":request["id"],"ok":true,"result":result})
                    )
                    .is_err()
                    {
                        break;
                    }
                }
                assert_eq!(queries, 4);
            });
            let options = crate::models::core::CoreIpcOptions {
                socket: Some(path.to_string_lossy().into_owned()),
                timeout_ms: Some(1000),
            };
            let control = BoundControl::connect(options.clone()).await.unwrap();
            let identity = control.identity().await.unwrap();
            let binding = RuntimeBinding {
                runtime: identity.clone(),
                profile_id: "actual-profile".into(),
                client_generation: 1,
            };
            let bridge = RuntimeBridge::default();
            bridge.bind(binding.clone()).unwrap();
            let caller = HostCaller::from_window_label("main").unwrap();
            let manager = znet_client_core::capability::Manager::default();
            let lease = znet_client_capabilities::host::operation(
                &manager,
                HOST_IDENTITY,
                znet_client_core::capability::Permission::new(
                    "runtime.query",
                    binding.permission_scope(),
                ),
                1,
                MAX_RESULT_BYTES,
                Duration::from_secs(5),
            )
            .unwrap();
            let auth = zero_api::AuthContext {
                subject: None,
                permissions: vec![zero_api::Permission::Read],
            };
            let result = bridge
                .call(
                    &control,
                    bridge.issue(&caller).unwrap(),
                    BridgeRequest::Query(zero_api::QueryRequest::Runtime(Default::default())),
                    &lease,
                    &auth,
                )
                .await;
            if replace_instance {
                assert!(matches!(result, Err(BridgeError::StaleBinding)));
            } else {
                let inspection = confirmed_inspection(identity, result.unwrap().receipt);
                assert_eq!(inspection.receipt.state, ReceiptState::Confirmed);
                let json = serde_json::to_string(&inspection).unwrap();
                assert!(json.contains("9007199254740993"));
                assert!(!json.contains("never-export-runtime-secret"));
                assert_eq!(inspection.vpn_observation, "unprovided");
            }
            crate::kernel::connection::reset_endpoint(
                &crate::kernel::protocol::endpoint_from_options(Some(&options)).unwrap(),
            );
            worker.join().unwrap();
        }
    }
}
