use crate::models::logs::LogLevel;

/// Remove ANSI escape codes (CSI sequences) from stderr output.
/// Matches patterns like ESC[...m, ESC[...K, etc.

/// Classify a core stderr line into a log level based on content heuristics.
fn classify_stderr_level(line: &str) -> LogLevel {
    let lower = line.to_ascii_lowercase();
    if lower.contains("error") || lower.contains("fatal") || lower.contains("panic") {
        LogLevel::Error
    } else if lower.contains("warn") {
        LogLevel::Warn
    } else {
        LogLevel::Info
    }
}

/// Parse a kernel log line into a structured level and fields.
///
/// The kernel uses a `tracing-subscriber` style format:
///   `2026-06-10T10:33:23.610135Z  INFO ipc client connected pipe=... active=3`
///
/// Extracts the timestamp and level, then converts `key=value` pairs
/// into JSON fields so the frontend can display them in a structured way.
fn is_structured_field_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn parse_structured_field_value(value: &str) -> serde_json::Value {
    if value.starts_with('"') && value.ends_with('"') {
        if let Ok(parsed) = serde_json::from_str::<String>(value) {
            return serde_json::Value::String(parsed);
        }
    }
    if let Ok(value) = value.parse::<i64>() {
        return serde_json::Value::Number(value.into());
    }
    if let Ok(value) = value.parse::<u64>() {
        return serde_json::Value::Number(value.into());
    }
    match value {
        "true" => serde_json::Value::Bool(true),
        "false" => serde_json::Value::Bool(false),
        _ => serde_json::Value::String(value.to_string()),
    }
}

fn structured_field_starts(message: &str) -> Vec<(usize, usize)> {
    let bytes = message.as_bytes();
    let mut starts = Vec::new();
    let mut index = 0;
    let mut in_quotes = false;
    let mut escaped = false;

    while index < bytes.len() {
        let byte = bytes[index];
        if in_quotes {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_quotes = false;
            }
            index += 1;
            continue;
        }
        if byte == b'"' {
            in_quotes = true;
            index += 1;
            continue;
        }

        let at_token_boundary = index == 0 || bytes[index - 1].is_ascii_whitespace();
        if at_token_boundary && (byte.is_ascii_alphanumeric() || byte == b'_') {
            let key_start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            if index < bytes.len() && bytes[index] == b'=' {
                starts.push((key_start, index));
                index += 1;
                continue;
            }
        }
        index += 1;
    }

    starts
}

pub(super) fn parse_kernel_log_line(line: &str) -> (LogLevel, serde_json::Value) {
    let mut fields = serde_json::Map::new();

    // Try to parse the tracing-subscriber format:
    //   <ISO 8601 timestamp>  <LEVEL> <spans...> <message key=value ...>
    let trimmed = line.trim();

    // Split off the ISO 8601 timestamp: YYYY-MM-DDTHH:MM:SS(.fff)?Z
    let (ts_rest, level, msg) = if trimmed.len() >= 20
        && trimmed.as_bytes().get(4) == Some(&b'-')
        && trimmed.as_bytes().get(10) == Some(&b'T')
    {
        // Find end of timestamp (next space)
        let ts_end = trimmed.find(' ').unwrap_or(trimmed.len());
        let ts = &trimmed[..ts_end];
        fields.insert(
            "timestamp".to_string(),
            serde_json::Value::String(ts.to_string()),
        );

        let after_ts = trimmed[ts_end..].trim_start();

        // Next token is the level: INFO, WARN, ERROR, DEBUG, TRACE
        let (lv, rest) = if after_ts.len() >= 4 {
            let level_end = after_ts.find(' ').unwrap_or(after_ts.len());
            (&after_ts[..level_end], after_ts[level_end..].trim_start())
        } else {
            ("INFO", after_ts)
        };
        fields.insert(
            "level".to_string(),
            serde_json::Value::String(lv.to_string()),
        );

        (true, lv, rest)
    } else {
        (false, "", trimmed)
    };

    let level = if ts_rest {
        match level {
            "ERROR" => LogLevel::Error,
            "WARN" => LogLevel::Warn,
            "DEBUG" | "TRACE" => LogLevel::Debug,
            _ => LogLevel::Info,
        }
    } else {
        classify_stderr_level(line)
    };

    // A tracing field value can contain spaces, either inside quotes (node
    // tags) or as an unquoted Display value (errors). A field therefore ends
    // at the next ASCII key= boundary, not at the next whitespace character.
    let field_starts = structured_field_starts(msg);
    let clean_msg = field_starts
        .first()
        .map_or(msg, |(start, _)| &msg[..*start])
        .trim();
    for (position, (field_start, equals)) in field_starts.iter().enumerate() {
        let key = &msg[*field_start..*equals];
        if !is_structured_field_key(key) {
            continue;
        }
        let value_end = field_starts
            .get(position + 1)
            .map_or(msg.len(), |(next_start, _)| *next_start);
        let value = msg[*equals + 1..value_end].trim();
        fields.insert(key.to_string(), parse_structured_field_value(value));
    }

    fields.insert(
        "message".to_string(),
        serde_json::Value::String(if clean_msg.is_empty() {
            msg.to_string()
        } else {
            clean_msg.to_string()
        }),
    );

    (level, serde_json::Value::Object(fields))
}

#[cfg(test)]
mod tests {
    use super::parse_kernel_log_line;
    use crate::models::logs::LogLevel;
    use crate::services::common::strip_ansi;

    #[test]
    fn strip_ansi_preserves_utf8_node_titles() {
        assert_eq!(
            strip_ansi("\u{1b}[32m🇸🇬 新加坡 IEPL\u{1b}[0m"),
            "🇸🇬 新加坡 IEPL"
        );
    }

    #[test]
    fn parse_kernel_log_line_keeps_plain_trailing_word() {
        let (level, fields) =
            parse_kernel_log_line("2026-06-10T10:33:23.610135Z INFO kernel started");

        assert_eq!(level, LogLevel::Info);
        assert_eq!(fields["message"], "kernel started");
        assert_eq!(fields["timestamp"], "2026-06-10T10:33:23.610135Z");
        assert_eq!(fields["level"], "INFO");
    }

    #[test]
    fn parse_kernel_log_line_extracts_fields_without_dropping_message_words() {
        let (_, fields) = parse_kernel_log_line(
            "2026-06-10T10:33:23.610135Z INFO ipc client connected pipe=\\\\.\\pipe\\zero-control active=3",
        );

        assert_eq!(fields["message"], "ipc client connected");
        assert_eq!(fields["pipe"], "\\\\.\\pipe\\zero-control");
        assert_eq!(fields["active"], 3);
    }

    #[test]
    fn parse_kernel_log_line_handles_unicode_message_without_panicking() {
        let (_, fields) = parse_kernel_log_line(
            "2026-06-10T10:33:23.610135Z INFO 节点测速完成 target=HK-01 latency_ms=88",
        );

        assert_eq!(fields["message"], "节点测速完成");
        assert_eq!(fields["target"], "HK-01");
        assert_eq!(fields["latency_ms"], 88);
    }

    #[test]
    fn parse_kernel_session_log_produces_typed_structured_fields() {
        let (_, fields) = parse_kernel_log_line(
            "2026-07-17T13:06:07.143223Z INFO session finished session_id=2 inbound_tag=\"socks-in\" outbound_tag=\"direct\" protocol=\"http\" network=\"tcp\" mode=\"rule\" target=Domain(\"api.github.com\") port=443 outcome=\"direct_relayed\" duration_ms=2339 bytes_up=966 bytes_down=987386",
        );

        assert_eq!(fields["message"], "session finished");
        assert_eq!(fields["session_id"], 2);
        assert_eq!(fields["inbound_tag"], "socks-in");
        assert_eq!(fields["port"], 443);
        assert_eq!(fields["duration_ms"], 2339);
        assert_eq!(fields["bytes_down"], 987386);
    }

    #[test]
    fn parse_kernel_log_line_preserves_spaced_tags_and_display_errors() {
        let (level, fields) = parse_kernel_log_line(
            "2026-07-22T01:52:03.985952Z WARN session failed session_id=93 inbound_tag=\"mixed-in\" outbound_tag=\"🇸🇬 新加坡 IEPL [0x11] [Std]\" protocol=\"http\" network=\"tcp\" mode=\"rule\" target=Domain(\"rank.similarweb.com\") port=443 stage=\"relay\" error=远程主机强迫关闭了一个现有的连接。 (os error 10054) upstream_server=\"example.com\" upstream_port=44330 duration_ms=12575",
        );

        assert_eq!(level, LogLevel::Warn);
        assert_eq!(fields["message"], "session failed");
        assert_eq!(fields["outbound_tag"], "🇸🇬 新加坡 IEPL [0x11] [Std]");
        assert_eq!(
            fields["error"],
            "远程主机强迫关闭了一个现有的连接。 (os error 10054)"
        );
        assert_eq!(fields["upstream_server"], "example.com");
        assert_eq!(fields["duration_ms"], 12575);
    }

    #[test]
    fn parse_kernel_log_line_keeps_non_field_equals_text_in_message() {
        let (_, fields) =
            parse_kernel_log_line("2026-06-10T10:33:23.610135Z INFO invalid-key=value retry later");

        assert_eq!(fields["message"], "invalid-key=value retry later");
        assert!(fields.get("invalid-key").is_none());
    }
}
