use super::{ensure_instance, Action, ControlInput};
use crate::errors::{AppError, AppResult};
use serde_json::{json, Value};

pub(super) fn validate_endpoint(row: &Value) -> AppResult<()> {
    let invalid = || AppError::internal("invalid endpoint V1 snapshot");
    for key in [
        "endpoint_id",
        "tag",
        "protocol",
        "core_instance_id",
        "state",
        "health",
        "state_source",
    ] {
        if row[key].as_str().is_none_or(str::is_empty) {
            return Err(invalid());
        }
    }
    for key in ["config_revision", "intent_revision", "observed_at_unix_ms"] {
        if row[key].as_u64().is_none() {
            return Err(invalid());
        }
    }
    for key in ["generation", "started_at_unix_ms"] {
        if row.get(key).is_none() || (!row[key].is_null() && row[key].as_u64().is_none()) {
            return Err(invalid());
        }
    }
    if row["enabled"].as_bool().is_none() {
        return Err(invalid());
    }
    for array in [
        &row["inbound_tags"],
        &row["outbound_tags"],
        &row["supported"]["operations"],
    ] {
        if !array
            .as_array()
            .is_some_and(|items| items.iter().all(Value::is_string))
        {
            return Err(invalid());
        }
    }
    for directions in [
        &row["allowed"],
        &row["effective"],
        &row["supported"]["directions"],
    ] {
        if directions["inbound"].as_bool().is_none() || directions["outbound"].as_bool().is_none() {
            return Err(invalid());
        }
    }
    if !row["counters"].as_object().is_some_and(|fields| {
        fields
            .values()
            .all(|value| value.is_null() || value.as_u64().is_some())
    }) {
        return Err(invalid());
    }
    Ok(())
}

pub(super) fn page_next(page: &Value, offset: u64, row_count: usize) -> AppResult<Option<u64>> {
    let invalid = || AppError::internal("invalid endpoint pagination");
    let total = page["total"].as_u64().ok_or_else(invalid)?;
    let end = offset.checked_add(row_count as u64).ok_or_else(invalid)?;
    if end > total || row_count > 1000 {
        return Err(invalid());
    }
    match page.get("next_offset") {
        Some(Value::Null) if end == total => Ok(None),
        Some(next) if row_count > 0 && next.as_u64() == Some(end) && end < total => Ok(Some(end)),
        _ => Err(invalid()),
    }
}

pub(super) fn confirmed(response: Value, input: &ControlInput) -> AppResult<Value> {
    let result = &response["result"];
    if response["accepted"] != true || result["applied"] != true || result["reconciled"] != true {
        return Err(AppError::core_response(response));
    }
    ensure_instance(&result["endpoint"], &input.core_instance_id)?;
    if result["endpoint"]["endpoint_id"].as_str() != Some(&input.endpoint_id) {
        return Err(AppError::internal(
            "endpoint acknowledgement identity mismatch",
        ));
    }
    validate_endpoint(&result["endpoint"])?;
    let endpoint = &result["endpoint"];
    let matches_intent = match &input.action {
        Action::SetState { enabled, .. } => endpoint["enabled"] == *enabled,
        Action::SetDirections { directions, .. } => endpoint["allowed"] == json!(directions),
        Action::Restart {} => true,
        Action::ClearOverrides {} => endpoint["state_source"] == "config",
    };
    let matches_persistence = match &input.action {
        Action::SetState { persistence, .. } | Action::SetDirections { persistence, .. } => {
            result["persistence"] == json!(persistence)
        }
        _ => true,
    };
    if !matches_persistence {
        return Err(AppError::internal(
            "endpoint acknowledgement does not confirm requested persistence",
        ));
    }
    if !matches_intent {
        return Err(AppError::internal(
            "endpoint acknowledgement does not match requested intent",
        ));
    }
    Ok(result["endpoint"].clone())
}
