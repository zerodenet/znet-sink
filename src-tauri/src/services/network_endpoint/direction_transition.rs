//! One client action using existing acknowledged kernel commands. Intermediate
//! stopped states never enter the client's persistent configuration preferences.
use super::{Action, ControlInput, Directions, Persistence};
use crate::errors::{AppError, AppResult};
use crate::models::{core::CoreIpcOptions, gui_core::GuiZeroCapabilities};
use serde_json::{json, Value};

trait Backend: Sync {
    fn execute(
        &self,
        input: ControlInput,
        revision: u64,
    ) -> impl std::future::Future<Output = AppResult<Value>> + Send;
    fn observe(&self, id: &str) -> impl std::future::Future<Output = AppResult<Value>> + Send;
}
struct Live(CoreIpcOptions);
impl Backend for Live {
    async fn execute(&self, input: ControlInput, revision: u64) -> AppResult<Value> {
        super::control_at_revision(input, self.0.clone(), Some(revision)).await
    }
    async fn observe(&self, id: &str) -> AppResult<Value> {
        let row = super::get(id, self.0.clone()).await?;
        super::wire::validate_endpoint(&row)?;
        Ok(row)
    }
}
fn requires_stop(row: &Value, directions: &Directions, caps: &GuiZeroCapabilities) -> bool {
    if row["state"] != "running" {
        return false;
    }
    let shrink_in = row["allowed"]["inbound"] == true && !directions.inbound;
    let shrink_out = row["allowed"]["outbound"] == true && !directions.outbound;
    if super::supports(caps, "network_endpoint_operation_capabilities_v1") {
        if let Some(live) = row
            .pointer("/supported/operation_capabilities/set_directions/live_direction_contraction")
        {
            return (shrink_in && live["inbound"] != true)
                || (shrink_out && live["outbound"] != true);
        }
    }
    let all = caps
        .global_limitations
        .iter()
        .any(|v| v == "endpoint_live_direction_contraction_requires_stop");
    let outbound = caps
        .global_limitations
        .iter()
        .any(|v| v == "endpoint_live_outbound_direction_contraction_requires_stop");
    (all && (shrink_in || shrink_out)) || (outbound && shrink_out)
}
pub(super) async fn apply(
    input: ControlInput,
    current: Value,
    caps: &GuiZeroCapabilities,
    options: CoreIpcOptions,
    revision: u64,
) -> AppResult<Value> {
    let Action::SetDirections { directions, .. } = &input.action else {
        return super::control_at_revision(input, options, Some(revision)).await;
    };
    if !requires_stop(&current, directions, caps) {
        return super::control_at_revision(input, options, Some(revision)).await;
    }
    if !super::supports(caps, "network_endpoint_control_preconditions_v1")
        || !current["supported"]["operations"]
            .as_array()
            .is_some_and(|ops| ops.iter().any(|v| v == "set_state"))
    {
        return Err(super::unsupported(
            "当前内核缺少安全切换方向的控制能力，请升级内核",
        ));
    }
    transition(&Live(options), current, directions.clone(), revision).await
}
fn request(row: &Value, action: Action) -> ControlInput {
    ControlInput {
        endpoint_id: row["endpoint_id"].as_str().unwrap().into(),
        core_instance_id: row["core_instance_id"].as_str().unwrap().into(),
        expected_intent_revision: row["intent_revision"].as_u64().unwrap(),
        action,
    }
}
fn state(enabled: bool) -> Action {
    Action::SetState {
        enabled,
        persistence: Persistence::RuntimeOnly,
    }
}
fn direction(directions: Directions) -> Action {
    Action::SetDirections {
        directions,
        persistence: Persistence::RuntimeOnly,
    }
}
fn same_scope(a: &Value, b: &Value) -> bool {
    [
        "endpoint_id",
        "core_instance_id",
        "config_revision",
        "tag",
        "protocol",
        "inbound_tags",
        "outbound_tags",
    ]
    .iter()
    .all(|key| a[key] == b[key])
}
fn unchanged(a: &Value, b: &Value) -> bool {
    same_scope(a, b)
        && [
            "intent_revision",
            "generation",
            "enabled",
            "allowed",
            "state",
        ]
        .iter()
        .all(|key| a[key] == b[key])
}
async fn step(
    backend: &impl Backend,
    last: &mut Value,
    action: Action,
    revision: u64,
) -> AppResult<()> {
    let next = backend.execute(request(last, action), revision).await?;
    super::wire::validate_endpoint(&next)?;
    if !same_scope(last, &next) {
        return Err(AppError::conflict(
            "endpoint",
            "",
            "方向切换期间内核或配置已变化",
        ));
    }
    *last = next;
    Ok(())
}
async fn transition(
    backend: &impl Backend,
    original: Value,
    requested: Directions,
    revision: u64,
) -> AppResult<Value> {
    let mut last = original.clone();
    let result: AppResult<()> = async {
        step(backend, &mut last, state(false), revision).await?;
        if last["state"] != "stopped" {
            return Err(AppError::internal("端点停止尚未确认，未继续修改方向"));
        }
        step(backend, &mut last, direction(requested), revision).await?;
        if last["enabled"] != false {
            return Err(AppError::internal("端点意外启用，未继续方向切换"));
        }
        step(
            backend,
            &mut last,
            state(original["enabled"].as_bool().unwrap()),
            revision,
        )
        .await
    }
    .await;
    if let Err(mut error) = result {
        // A disconnected client can observe the old snapshot while the kernel
        // is still finishing its transaction. That is not proof of rollback.
        let rejected = error.details.as_ref().is_some_and(|details| {
            details["ok"] == false
                && details
                    .pointer("/error/code")
                    .and_then(Value::as_str)
                    .is_some()
        });
        let recovery = if rejected {
            restore(backend, &original, &last, revision).await
        } else {
            Err(AppError::internal("操作结果未确定，不自动恢复或重发"))
        };
        let description = match &recovery {
            Ok(()) => "方向切换失败，已恢复原状态",
            Err(_) => "方向切换未完成；状态未能确认恢复，请查看当前端点状态",
        };
        error.message = format!("{description}：{}", error.message);
        error.details = Some(json!({"cause":error.details,"recovery": match recovery {
            Ok(()) => json!({"restored":true}),
            Err(recovery) => json!({"restored":false,"code":recovery.code,"message":recovery.message}),
        }}));
        return Err(error);
    }
    Ok(last)
}
async fn restore(
    backend: &impl Backend,
    original: &Value,
    last: &Value,
    revision: u64,
) -> AppResult<()> {
    let mut observed = backend
        .observe(original["endpoint_id"].as_str().unwrap())
        .await?;
    if !unchanged(last, &observed) {
        // Never adopt a newer revision to undo another actor's intent, or
        // retry a mutation whose confirmation was lost.
        return Err(AppError::conflict(
            "endpoint",
            "",
            "端点观测已变化，未覆盖新的状态",
        ));
    }
    if observed["allowed"] != original["allowed"] {
        let old = serde_json::from_value(original["allowed"].clone())
            .map_err(|e| AppError::internal(e.to_string()))?;
        step(backend, &mut observed, direction(old), revision).await?;
    }
    if observed["enabled"] != original["enabled"] {
        step(
            backend,
            &mut observed,
            state(original["enabled"].as_bool().unwrap()),
            revision,
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "direction_transition_tests.rs"]
mod tests;
