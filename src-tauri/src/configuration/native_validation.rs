//! Text-editor admission for ordinary Zero configuration material.
//!
//! This is schema/reference validation, not runtime compatibility or permission
//! to read credentials, contact servers, create listeners, or apply a profile.
//! The pinned native validator compiles routing/DNS and can read rule/GeoIP
//! files. Inspect its public typed schema first and deny those dependencies
//! before calling it. Keep the original JSON value, including omitted defaults
//! and lossless u64 values; never export the normalized/materialized schema.
use crate::errors::{AppError, AppResult};
use serde_json::{json, Value};

pub(crate) const VALIDATION_SCOPE: &str = "schema-and-references";
const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;

pub(crate) fn validate_text(text: &str) -> AppResult<Value> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(failure("", "text_budget_exceeded", None));
    }
    // Match the native parser: allow one leading BOM, not arbitrary trimming.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let value: Value = serde_json::from_str(text).map_err(json_error)?;
    // Deserialize the original text, not Value: native duplicate-field errors
    // must not be erased by Value's last-key-wins object construction.
    let typed: zero_config::RuntimeConfig = serde_json::from_str(text).map_err(json_error)?;
    if !typed.route.rule_sets.is_empty() {
        return Err(failure(
            "route.rule_sets",
            "dependency_authorization_required",
            None,
        ));
    }
    if typed.route.geoip_database.is_some() {
        return Err(failure(
            "route.geoip_database",
            "dependency_authorization_required",
            None,
        ));
    }
    // At pinned zero-config e74561f, these are the complete filesystem-read
    // sources reachable from RuntimeConfig::parse. TLS, env credentials,
    // mutable output files and compiled protocol support are runtime concerns;
    // acceptance here deliberately makes no claim about them.
    zero_config::RuntimeConfig::parse(text).map_err(native_error)?;
    Ok(value)
}

fn failure(path: &'static str, reason: &'static str, position: Option<(usize, usize)>) -> AppError {
    let (line, column) = position.map_or((None, None), |(l, c)| (Some(l), Some(c)));
    let message = if reason == "dependency_authorization_required" {
        "此配置的规则集或 GeoIP 需要文件授权；当前文本编辑入口不能读取这些材料，草稿保留"
    } else if reason == "text_budget_exceeded" {
        "配置文本超过16MiB预算，草稿保留"
    } else {
        "Zero 原生配置的语法、字段、类型或引用不合法，草稿保留"
    };
    AppError {
        code: "native_validation_failed",
        message: message.into(),
        details: Some(json!({
            "validation_scope": VALIDATION_SCOPE,
            "errors": [{"field_path": path, "reasonCode": reason,
                "message": message, "line": line, "column": column}]
        })),
    }
}

fn json_error(error: serde_json::Error) -> AppError {
    // Never expose serde's message: unknown names, enum values and input text
    // can contain credentials or URL tokens. Positions contain no material.
    failure(
        "",
        "invalid_json_or_schema",
        Some((error.line(), error.column())),
    )
}

fn native_error(error: zero_config::ConfigError) -> AppError {
    use zero_config::ConfigError::*;
    let (path, reason) = match error {
        ParseConfig(error) => return json_error(error),
        UnsupportedSchemaVersion { .. } => ("schema_version", "unsupported_schema_version"),
        InvalidInbound(_) | DuplicateInboundListen { .. } => ("inbounds", "invalid_inbound"),
        InvalidOutbound(_) => ("outbounds", "invalid_outbound"),
        InvalidOutboundGroup(_) => ("outbound_groups", "invalid_outbound_group"),
        InvalidRuntime(_) => ("runtime", "invalid_runtime"),
        InvalidApi(_) => ("api", "invalid_api"),
        InvalidMode(_) => ("mode", "invalid_mode"),
        InvalidDns(_) => ("runtime.dns", "invalid_dns"),
        InvalidRuleSet(_) | UndefinedRuleSetTag { .. } => ("route.rule_sets", "invalid_rule_set"),
        InvalidRouteAction(_) | InvalidRuleCondition(_) | UndefinedRouteTargetTag { .. } => {
            ("route", "invalid_route_or_reference")
        }
        EmptyTag { .. } | DuplicateTag { .. } | DuplicateRouteTargetTag { .. } => {
            ("", "invalid_or_duplicate_tag")
        }
        // Unexpected file access must fail closed as well, without echoing its
        // path or the OS message. Never downgrade it to accepted schema.
        ReadConfig { .. } | ReadRuleSet { .. } => ("", "dependency_authorization_required"),
    };
    failure(path, reason, None)
}

#[cfg(test)]
#[path = "native_validation_tests.rs"]
mod tests;
