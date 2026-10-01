//! Preserve every 64-bit field at the Rust -> JavaScript boundary.
use crate::errors::{AppError, AppResult};
use serde_json::Value;

pub(crate) fn exact(mut value: Value) -> Value {
    fn visit(value: &mut Value, name: &str, depth: usize) {
        match value {
            Value::Number(number) if !(depth == 1 && matches!(name, "total" | "next_offset")) => {
                if let Some(number) = number.as_u64() {
                    *value = Value::String(number.to_string());
                }
            }
            Value::Object(object) => {
                for (name, child) in object {
                    visit(child, name, depth + 1);
                }
            }
            Value::Array(array) => {
                for child in array {
                    visit(child, "", depth + 1);
                }
            }
            _ => {}
        }
    }
    visit(&mut value, "", 0);
    value
}
pub(crate) fn snapshot(value: &Value) -> AppResult<()> {
    for key in ["scope", "planes", "activity"] {
        if value.get(key).is_none() {
            return Err(AppError::internal(format!("missing traffic {key}")));
        }
    }
    if value["core_instance_id"].as_str().is_none_or(str::is_empty)
        || value["stats_epoch"].as_str().is_none_or(str::is_empty)
        || value["sampled_at_monotonic_ns"].as_u64().is_none()
        || value["config_revision"].as_u64().is_none()
    {
        return Err(AppError::internal(
            "invalid traffic snapshot identity or clock",
        ));
    }
    Ok(())
}
