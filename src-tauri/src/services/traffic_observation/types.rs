use crate::errors::{AppError, AppResult};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Scope {
    Global,
    Inbound {
        tag: String,
    },
    Outbound {
        tag: String,
    },
    Endpoint {
        endpoint_id: String,
    },
    Peer {
        endpoint_id: String,
        peer_id: String,
    },
}
impl Scope {
    pub fn wire(&self) -> Value {
        match self {
            Self::Global => json!({"kind":"global"}),
            Self::Inbound { tag } => json!({"kind":"inbound","tag":tag}),
            Self::Outbound { tag } => json!({"kind":"outbound","tag":tag}),
            Self::Endpoint { endpoint_id } => json!({"kind":"endpoint","endpoint_id":endpoint_id}),
            Self::Peer {
                endpoint_id,
                peer_id,
            } => json!({"kind":"peer","endpoint_id":endpoint_id,"peer_id":peer_id}),
        }
    }
}
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ListInput {
    #[serde(default)]
    pub offset: usize,
    pub limit: Option<usize>,
    #[serde(default)]
    pub scopes: Vec<Scope>,
    pub expected_core_instance_id: Option<String>,
    pub expected_config_revision: Option<String>,
    pub expected_registry_revision: Option<String>,
}
impl ListInput {
    pub fn wire(self) -> AppResult<Value> {
        if self.scopes.len() > 256 || self.limit.is_some_and(|v| !(1..=256).contains(&v)) {
            return Err(AppError::invalid_argument(
                "traffic query exceeds the page/selection limit",
            ));
        }
        let mut value = json!({"offset":self.offset,"limit":self.limit.unwrap_or(64),"scopes":self.scopes.iter().map(Scope::wire).collect::<Vec<_>>()});
        if let Some(id) = self.expected_core_instance_id {
            value["expected_core_instance_id"] = json!(id);
        }
        for (key, number) in [
            ("expected_config_revision", self.expected_config_revision),
            (
                "expected_registry_revision",
                self.expected_registry_revision,
            ),
        ] {
            if let Some(number) = number {
                value[key] = json!(decimal(&number)?);
            }
        }
        Ok(value)
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResetTarget {
    pub scope: Scope,
    pub expected_stats_epoch: String,
    pub expected_generation: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResetInput {
    pub expected_core_instance_id: String,
    pub operation_id: String,
    pub targets: Vec<ResetTarget>,
}
impl ResetInput {
    pub fn wire(&self) -> AppResult<Value> {
        let mut identities = std::collections::HashSet::new();
        if self.expected_core_instance_id.is_empty()
            || self.operation_id.is_empty()
            || !(1..=256).contains(&self.targets.len())
        {
            return Err(AppError::invalid_argument(
                "statistics reset needs an instance, operation and 1..256 explicit scopes",
            ));
        }
        let targets=self.targets.iter().map(|target| {
            if !identities.insert(&target.scope) || target.expected_stats_epoch.is_empty() { return Err(AppError::invalid_argument("duplicate scope or missing statistics epoch")); }
            let mut value=json!({"scope":target.scope.wire(),"expected_stats_epoch":target.expected_stats_epoch});
            if let Some(generation)=&target.expected_generation { value["expected_generation"]=json!(decimal(generation)?); }
            Ok(value)
        }).collect::<AppResult<Vec<_>>>()?;
        Ok(
            json!({"expected_core_instance_id":self.expected_core_instance_id,"operation_id":self.operation_id,"targets":targets}),
        )
    }
}
fn decimal(value: &str) -> AppResult<u64> {
    if value.is_empty() || !value.bytes().all(|v| v.is_ascii_digit()) {
        return Err(AppError::invalid_argument(
            "invalid unsigned statistics revision",
        ));
    }
    value
        .parse()
        .map_err(|_| AppError::invalid_argument("statistics revision exceeds u64"))
}
