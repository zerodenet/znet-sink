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

pub(crate) fn sensitive(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        if sensitive_key(key) {
                            Value::String("[redacted]".into())
                        } else {
                            sensitive(value)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(sensitive).collect()),
        Value::String(value) => Value::String(text(value)),
        value => value.clone(),
    }
}

pub(crate) fn text(value: &str) -> String {
    const SCHEMES: &[&str] = &[
        "https://",
        "http://",
        "hysteria2://",
        "ss://",
        "trojan://",
        "vless://",
        "vmess://",
    ];
    let mut output = String::with_capacity(value.len());
    let mut remaining = value;
    while let Some(index) = SCHEMES
        .iter()
        .filter_map(|scheme| remaining.find(scheme))
        .min()
    {
        output.push_str(&remaining[..index]);
        output.push_str("[redacted-url]");
        let url = &remaining[index..];
        let end = url
            .char_indices()
            .find_map(|(offset, character)| {
                (offset > 0
                    && (character.is_whitespace()
                        || matches!(character, '"' | '\'' | '<' | '>' | ')' | ']' | '}')))
                .then_some(offset)
            })
            .unwrap_or(url.len());
        remaining = &url[end..];
    }
    output.push_str(remaining);
    output
}

fn sensitive_key(key: &str) -> bool {
    let key: String = key
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .collect();
    matches!(
        key.as_str(),
        "auth"
            | "authorization"
            | "config"
            | "content"
            | "cookie"
            | "credentials"
            | "headers"
            | "password"
            | "privatekey"
            | "psk"
            | "secret"
            | "token"
            | "uri"
            | "url"
            | "uuid"
    ) || ["password", "secret", "token", "url", "privatekey", "apikey"]
        .iter()
        .any(|suffix| key.ends_with(suffix))
}

#[cfg(test)]
#[path = "redaction_tests.rs"]
mod tests;
