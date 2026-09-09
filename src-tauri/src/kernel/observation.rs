//! Platform bridge for the transport-independent observation client.
use serde_json::{json, Value};
use znet_engine_client::{
    Binding, FlowFilter, ObservationClient, ObservationError, QueryTransport,
};

use super::{
    connection::{self, MultiplexedConnection},
    protocol,
    zero::parsing,
};
use crate::errors::{AppError, AppResult};
use crate::models::gui_core::{GuiConnection, GuiConnectionList, GuiConnectionListOptions};

pub struct PinnedTransport {
    binding: Binding,
    connection: MultiplexedConnection,
}

impl QueryTransport for PinnedTransport {
    type Error = AppError;
    async fn query(&self, request: Value) -> AppResult<Value> {
        let call = protocol::request_on_connection(
            json!({"type": "query", "request": request}),
            &self.binding,
            self.connection.clone(),
        )
        .await?;
        parsing::unwrap_call_result(call.response, call.error)
    }
}

pub struct FlowObservation {
    client: ObservationClient<PinnedTransport>,
}

impl FlowObservation {
    pub async fn connect(binding: Binding) -> AppResult<Self> {
        let target = binding.clone();
        let connection = tauri::async_runtime::spawn_blocking(move || {
            connection::get_or_connect(target.endpoint, target.timeout)
        })
        .await
        .map_err(|e| AppError::internal(format!("observation connect failed: {e}")))??;
        Ok(Self::from_connection(binding, connection))
    }

    pub fn from_connection(binding: Binding, connection: MultiplexedConnection) -> Self {
        Self {
            client: ObservationClient::new(PinnedTransport {
                binding,
                connection,
            }),
        }
    }

    pub async fn active(
        &self,
        options: Option<GuiConnectionListOptions>,
    ) -> AppResult<GuiConnectionList> {
        let filter = filter(options);
        let value = self.client.active(&filter).await.map_err(map_error)?;
        Ok(parsing::parse_connection_list(&value, filter.limit()))
    }

    pub async fn recent(
        &self,
        options: Option<GuiConnectionListOptions>,
    ) -> AppResult<GuiConnectionList> {
        let filter = filter(options);
        let value = self.client.recent(&filter).await.map_err(map_error)?;
        Ok(parsing::parse_connection_list(&value, filter.limit()))
    }

    pub async fn detail(&self, flow_id: &str) -> AppResult<GuiConnection> {
        let value = self.client.detail(flow_id).await.map_err(map_error)?;
        parsing::parse_connection(&value)
            .ok_or_else(|| AppError::invalid_argument("core returned invalid flow"))
    }

    pub async fn snapshot(&self) -> AppResult<Value> {
        let snapshot = self.client.snapshot().await.map_err(map_error)?;
        Ok(json!({
            "runtime": snapshot.runtime,
            "stats": snapshot.stats,
            "policies": snapshot.policies,
            "connections": parsing::parse_connection_list(&snapshot.connections, 500),
        }))
    }
}

fn filter(options: Option<GuiConnectionListOptions>) -> FlowFilter {
    options
        .map(|o| FlowFilter {
            limit: o.limit,
            inbound_tag: o.inbound_tag,
            principal_key: o.principal_key,
        })
        .unwrap_or_default()
}

fn map_error(error: ObservationError<AppError>) -> AppError {
    match error {
        ObservationError::Transport(error) => error,
        ObservationError::InvalidInput(message) => AppError::invalid_argument(message),
        ObservationError::InvalidResponse(message) => AppError::internal(message),
        ObservationError::RuntimeChanged => AppError::conflict(
            "observation",
            "runtime",
            "runtime changed while reading observation baseline",
        ),
    }
}
