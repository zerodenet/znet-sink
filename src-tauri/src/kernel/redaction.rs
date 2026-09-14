//! Configuration material never belongs in debug rings, persisted traces or previews.
use serde_json::{Map, Value};
pub(crate) fn frame(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        if key == "config" {
                            Value::String("[configuration redacted]".into())
                        } else {
                            frame(value)
                        },
                    )
                })
                .collect::<Map<_, _>>(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(frame).collect()),
        value => value.clone(),
    }
}
pub(crate) fn preview(bytes: &[u8]) -> String {
    // Unknown/malformed envelopes are never copied as raw bytes to a text log.
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return "[unparseable control frame]".into();
    };
    let text = frame(&value).to_string();
    let mut preview: String = text.chars().take(200).collect();
    if text.chars().count() > 200 {
        preview.push('…');
    }
    preview
}

#[cfg(test)]
#[path = "redaction_tests.rs"]
mod tests;
