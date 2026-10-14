//! Bound configuration control. Recovery never rediscovers an endpoint midway
//! through a confirmation or replays old flow IDs on a replacement connection.
use super::{
    connection::{self, MultiplexedConnection},
    protocol,
    zero::{commands, parsing, queries},
};
use crate::errors::{AppError, AppResult};
use crate::models::core::CoreIpcOptions;
use serde_json::{json, Value};
use znet_engine_client::{configuration::RuntimeIdentity, Binding};

#[derive(Clone)]
pub(crate) struct BoundControl {
    binding: Binding,
    connection: MultiplexedConnection,
}
impl BoundControl {
    pub(crate) async fn connect(options: CoreIpcOptions) -> AppResult<Self> {
        let binding = Binding {
            endpoint: protocol::endpoint_from_options(Some(&options))?,
            timeout: protocol::timeout_from_options(Some(&options))?,
        };
        let target = binding.clone();
        let connection = tauri::async_runtime::spawn_blocking(move || {
            connection::get_or_connect(target.endpoint, target.timeout)
        })
        .await
        .map_err(|error| AppError::internal(format!("config connection failed: {error}")))??;
        Ok(Self {
            binding,
            connection,
        })
    }
    pub(crate) async fn call(&self, frame: Value) -> AppResult<Value> {
        let result =
            protocol::request_on_connection(frame, &self.binding, self.connection.clone()).await?;
        parsing::unwrap_call_result(result.response, result.error)
    }
    pub(crate) async fn identity(&self) -> AppResult<RuntimeIdentity> {
        let value = self
            .call(json!({"type":"query","request":{"runtime":{}}}))
            .await?;
        queries::parse_runtime_identity(value.get("runtime").unwrap_or(&value))
    }
    pub(crate) async fn tun_status(&self) -> AppResult<crate::models::zero_runtime::GuiTunStatus> {
        let value = self
            .call(json!({"type":"query","request":{"tun_status":{}}}))
            .await?;
        super::zero::runtime::parse_tun_status(value.get("tun_status").unwrap_or(&value))
    }
    pub(crate) async fn apply(&self, config: Value) -> AppResult<RuntimeIdentity> {
        let result = self
            .call(json!({"type":"command","method":"config.apply_runtime","params":{"config":config}}))
            .await?;
        commands::ensure_config_apply_accepted(&result)?;
        if result
            .pointer("/result/persistence")
            .or_else(|| result.get("persistence"))
            .and_then(Value::as_str)
            != Some("runtime_only")
        {
            return Err(AppError {
                code: "config_apply_uncertain",
                message: "内核未确认仅内存应用配置，请核对内核版本及运行状态".into(),
                details: None,
            });
        }
        queries::config_apply_identity(&result).map_err(|mut error| {
            error.code = "config_apply_uncertain";
            error.message = format!("配置应用结果无法确认，请刷新运行状态：{}", error.message);
            error
        })
    }
    pub(crate) async fn all_flow_ids(&self) -> AppResult<Vec<String>> {
        let result = self
            .call(json!({"type":"query","request":{"active_flows":{"filter":{}}}}))
            .await?;
        Ok(
            parsing::parse_connection_list(result.get("active_flows").unwrap_or(&result), u32::MAX)
                .items
                .into_iter()
                .map(|flow| flow.flow_id)
                .collect(),
        )
    }
    pub(crate) async fn close_flow(&self, flow_id: &str) -> AppResult<()> {
        match self
            .call(json!({"type":"command","method":"flows.close","params":{"flow_id":flow_id}}))
            .await
        {
            Ok(_) => Ok(()),
            Err(error) if commands::is_flow_already_completed_error(&error) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

impl znet_engine_client::flow_cleanup::FlowControl for BoundControl {
    type Error = AppError;
    async fn identity(&self) -> AppResult<RuntimeIdentity> {
        BoundControl::identity(self).await
    }
    async fn close(&self, flow_id: &str) -> AppResult<()> {
        self.close_flow(flow_id).await
    }
}

impl znet_engine_client::QueryTransport for BoundControl {
    type Error = AppError;
    async fn query(&self, request: Value) -> AppResult<Value> {
        self.call(json!({"type":"query","request":request})).await
    }
}

impl znet_engine_client::bridge::BridgeTransport for BoundControl {
    async fn command(&self, request: zero_api::CommandRequest) -> AppResult<Value> {
        let mut frame = serde_json::to_value(request)
            .map_err(|_| AppError::invalid_argument("控制请求编码失败"))?;
        frame["type"] = json!("command");
        self.call(frame).await
    }
    fn failure_is_rejection(&self, error: &AppError) -> bool {
        let code = error
            .details
            .as_ref()
            .and_then(|details| details.pointer("/error/code"))
            .and_then(Value::as_str)
            .unwrap_or(error.code);
        matches!(
            code,
            "invalid_argument"
                | "permission_denied"
                | "feature_disabled"
                | "unsupported"
                | "not_found"
                | "conflict"
        )
    }
}
#[cfg(all(test, unix))]
#[path = "configuration_tests.rs"]
mod tests;
