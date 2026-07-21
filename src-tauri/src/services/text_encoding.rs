use std::borrow::Cow;

use serde_json::Value;

/// Repair text that was UTF-8 once, but whose bytes were interpreted as
/// ISO-8859-1 code points before being encoded as UTF-8 again. This exact
/// shape appears in some Windows kernel stderr output (for example
/// `ð\u{9f}\u{87}¸` instead of a flag emoji).
pub(crate) fn repair_utf8_mojibake(value: &str) -> Cow<'_, str> {
    let has_c1_control = value.chars().any(|ch| ('\u{80}'..='\u{9f}').contains(&ch));
    let has_utf8_lead_marker = value
        .chars()
        .any(|ch| matches!(ch, 'Ã' | 'Â' | 'ð' | 'æ' | 'å' | 'ç' | 'è' | 'é' | 'ä'));
    if !has_c1_control && !has_utf8_lead_marker {
        return Cow::Borrowed(value);
    }

    let mut bytes = Vec::with_capacity(value.len());
    for ch in value.chars() {
        let codepoint = ch as u32;
        if codepoint > u8::MAX as u32 {
            return Cow::Borrowed(value);
        }
        bytes.push(codepoint as u8);
    }

    let Ok(repaired) = String::from_utf8(bytes) else {
        return Cow::Borrowed(value);
    };
    let repaired_c1_controls = repaired
        .chars()
        .filter(|ch| ('\u{80}'..='\u{9f}').contains(ch))
        .count();
    let original_c1_controls = value
        .chars()
        .filter(|ch| ('\u{80}'..='\u{9f}').contains(ch))
        .count();
    let recovered_non_latin = repaired.chars().any(|ch| ch as u32 > u8::MAX as u32);

    if repaired_c1_controls < original_c1_controls || (has_utf8_lead_marker && recovered_non_latin)
    {
        Cow::Owned(repaired)
    } else {
        Cow::Borrowed(value)
    }
}

pub(crate) fn repair_string(value: &mut String) {
    if let Cow::Owned(repaired) = repair_utf8_mojibake(value) {
        *value = repaired;
    }
}

pub(crate) fn repair_json_strings(value: &mut Value) {
    match value {
        Value::String(text) => repair_string(text),
        Value::Array(items) => {
            for item in items {
                repair_json_strings(item);
            }
        }
        Value::Object(fields) => {
            for value in fields.values_mut() {
                repair_json_strings(value);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{repair_json_strings, repair_utf8_mojibake};
    use serde_json::json;

    fn simulate_single_byte_misdecode(value: &str) -> String {
        value.as_bytes().iter().copied().map(char::from).collect()
    }

    #[test]
    fn repairs_flags_and_chinese_from_double_encoded_utf8() {
        let expected = "selected=🇸🇬 新加坡 IEPL";
        let mojibake = simulate_single_byte_misdecode(expected);

        assert_eq!(repair_utf8_mojibake(&mojibake), expected);
    }

    #[test]
    fn keeps_valid_unicode_and_ascii_unchanged() {
        assert!(matches!(
            repair_utf8_mojibake("selected=🇸🇬 新加坡 IEPL"),
            std::borrow::Cow::Borrowed(_)
        ));
        assert!(matches!(
            repair_utf8_mojibake("kernel started"),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    #[test]
    fn repairs_nested_log_fields() {
        let mut value = json!({
            "message": simulate_single_byte_misdecode("远程主机关闭连接"),
            "members": [simulate_single_byte_misdecode("🇯🇵 日本")]
        });

        repair_json_strings(&mut value);

        assert_eq!(value["message"], "远程主机关闭连接");
        assert_eq!(value["members"][0], "🇯🇵 日本");
    }
}
