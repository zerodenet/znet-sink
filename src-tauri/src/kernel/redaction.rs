//! Configuration and credentials never belong in diagnostic records.
use serde_json::Value;

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

pub(crate) fn sensitive_key(key: &str) -> bool {
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
