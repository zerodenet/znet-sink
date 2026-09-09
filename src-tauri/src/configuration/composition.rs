//! Explicit composition inputs. No AppState, filesystem reads, IPC, or persistence.
use super::{dns, CompositionReport};
use crate::errors::AppResult;
use crate::models::app_config::AppConfig;
use crate::services::{bypass, policy_selection, proxy_config, url_test};
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) struct Inputs {
    pub source_profile_id: Option<String>,
    pub app: AppConfig,
    pub supports_tolerance: bool,
    pub selections: BTreeMap<String, String>,
}
pub(crate) struct Candidate {
    pub config: Value,
    pub report: CompositionReport,
}

pub(crate) fn finalize(base: &Value, mut config: Value, inputs: &Inputs) -> AppResult<Candidate> {
    let mut report = CompositionReport {
        source_profile_id: inputs.source_profile_id.clone(),
        ..Default::default()
    };
    report.compare(
        "common_rules",
        &["/route/rule_sets", "/route/rules"],
        base,
        &config,
    );
    report.apply("local_listener", &["/inbounds"], &mut config, |config| {
        proxy_config::project_managed_endpoint(config, &inputs.app.local_proxy)
    })?;
    report.apply("client_tun", &["/runtime/tun"], &mut config, |config| {
        if let Some(runtime) = config.get_mut("runtime").and_then(Value::as_object_mut) {
            runtime.remove("tun");
        }
        Ok(())
    })?;
    report.apply("global_dns", &["/runtime/dns"], &mut config, |config| {
        dns::apply_global_dns(config, &inputs.app.dns)
    })?;
    report.apply(
        "urltest_override",
        &["/runtime/latency_test_url", "/outbound_groups"],
        &mut config,
        |config| {
            url_test::apply_url(config, &inputs.app.url_test.url)?;
            if inputs.supports_tolerance {
                url_test::apply_default_tolerance(config, inputs.app.url_test.tolerance_ms)?;
            }
            Ok(())
        },
    )?;
    report.apply(
        "saved_selections",
        &["/outbound_groups"],
        &mut config,
        |config| {
            policy_selection::apply_selections(config, &inputs.selections);
            Ok(())
        },
    )?;
    report.apply(
        "bypass",
        &[
            "/route/bypass",
            "/runtime/tun/exclude_cidrs",
            "/runtime/dns",
        ],
        &mut config,
        |config| bypass::apply(config, &inputs.app),
    )?;
    Ok(Candidate { config, report })
}
