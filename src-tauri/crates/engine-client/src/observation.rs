use serde_json::{json, Map, Value};
use std::future::Future;

/// The adapter must stay bound to one connection. If that connection dies,
/// return an error; acquiring a replacement belongs to the subscription owner.
pub trait QueryTransport: Send + Sync {
    type Error: Send;
    fn query(&self, request: Value) -> impl Future<Output = Result<Value, Self::Error>> + Send;
}

#[derive(Debug, PartialEq)]
pub enum ObservationError<E> {
    Transport(E),
    InvalidInput(&'static str),
    InvalidResponse(&'static str),
    RuntimeChanged,
}

#[derive(Clone, Default)]
pub struct FlowFilter {
    pub limit: Option<u32>,
    pub inbound_tag: Option<String>,
    pub principal_key: Option<String>,
}

impl FlowFilter {
    pub fn limit(&self) -> u32 {
        self.limit.unwrap_or(100).clamp(1, 500)
    }

    fn request(&self) -> Value {
        let mut filter = Map::new();
        for (key, value) in [
            ("inbound_tag", &self.inbound_tag),
            ("principal_key", &self.principal_key),
        ] {
            if let Some(value) = value.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                filter.insert(key.into(), json!(value));
            }
        }
        json!({"limit": self.limit(), "filter": filter})
    }
}

pub struct Snapshot {
    pub runtime: Value,
    pub stats: Option<Value>,
    pub policies: Option<Value>,
    pub connections: Value,
}

pub struct ObservationClient<T> {
    transport: T,
}

impl<T: QueryTransport> ObservationClient<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    async fn query(&self, name: &str, payload: Value) -> Result<Value, ObservationError<T::Error>> {
        let result = self
            .transport
            .query(json!({name: payload}))
            .await
            .map_err(ObservationError::Transport)?;
        // Modern tagged QueryResponse and the older flat response are both supported.
        Ok(result.get(name).cloned().unwrap_or(result))
    }

    pub async fn active(&self, filter: &FlowFilter) -> Result<Value, ObservationError<T::Error>> {
        self.query("active_flows", filter.request()).await
    }

    pub async fn recent(&self, filter: &FlowFilter) -> Result<Value, ObservationError<T::Error>> {
        self.query("recent_flows", filter.request()).await
    }

    pub async fn detail(&self, flow_id: &str) -> Result<Value, ObservationError<T::Error>> {
        let flow_id = flow_id.trim();
        if flow_id.is_empty() {
            return Err(ObservationError::InvalidInput("flowId must not be empty"));
        }
        self.query("flow", json!({"flow_id": flow_id})).await
    }

    /// Publish a baseline only if its mandatory queries belong to the same
    /// runtime/configuration. Optional stats/policy failures remain visible as
    /// missing data, never as fabricated empty authoritative snapshots.
    pub async fn snapshot(&self) -> Result<Snapshot, ObservationError<T::Error>> {
        let runtime = self.query("runtime", json!({})).await?;
        let before = identity(&runtime).ok_or(ObservationError::InvalidResponse(
            "runtime identity missing",
        ))?;
        let stats = self.query("stats", json!({})).await.ok();
        let policies = self.query("policies", json!({})).await.ok();
        let connections = self
            .active(&FlowFilter {
                limit: Some(500),
                ..Default::default()
            })
            .await?;
        let current = self.query("runtime", json!({})).await?;
        let after = identity(&current).ok_or(ObservationError::InvalidResponse(
            "runtime identity missing",
        ))?;
        if before != after {
            return Err(ObservationError::RuntimeChanged);
        }
        Ok(Snapshot {
            runtime: current,
            stats,
            policies,
            connections,
        })
    }
}

fn identity(value: &Value) -> Option<(&str, u64)> {
    let id = value
        .get("core_instance_id")
        .or_else(|| value.get("coreInstanceId"))?
        .as_str()?;
    if id.trim().is_empty() {
        return None;
    }
    let revision = value
        .get("config_revision")
        .or_else(|| value.get("configRevision"))?;
    let revision = revision
        .as_u64()
        .or_else(|| revision.as_str()?.parse().ok())?;
    Some((id, revision))
}
