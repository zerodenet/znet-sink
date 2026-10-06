use crate::models::zero_runtime::GuiTunStatus;
use serde_json::Value;

pub(super) fn matches(tun: &GuiTunStatus, params: &Value) -> bool {
    tun.healthy && matches_plan(tun, params)
}

pub(super) fn matches_plan(tun: &GuiTunStatus, params: &Value) -> bool {
    let cidr = |a: &str, b: &str| {
        crate::services::kernel_settings::validate_cidr(a, "TUN")
            .ok()
            .zip(crate::services::kernel_settings::validate_cidr(b, "TUN").ok())
            .is_some_and(|(a, b)| a == b)
    };
    let routes = |actual: Option<&[String]>, key: &str| {
        actual.zip(params[key].as_array()).is_some_and(|(a, b)| {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(a, b)| b.as_str().is_some_and(|b| cidr(a, b)))
        })
    };
    tun.enabled
        && !tun.managed_by_config
        && tun
            .addr
            .as_deref()
            .zip(params["addr"].as_str())
            .is_some_and(|(a, b)| cidr(a, b))
        && tun.mtu.map(u64::from) == params["mtu"].as_u64()
        && tun.tag.as_deref() == params["tag"].as_str()
        && params["name"]
            .as_str()
            .is_none_or(|v| tun.name.as_deref() == Some(v))
        && Some(tun.auto_route) == params["auto_route"].as_bool()
        && Some(tun.strict_route) == params["strict_route"].as_bool()
        && Some(tun.dual_stack) == params["dual_stack"].as_bool()
        && Some(tun.dns_hijack) == params["dns_hijack"].as_bool()
        && routes(tun.include_cidrs.as_deref(), "include_cidrs")
        && routes(tun.exclude_cidrs.as_deref(), "exclude_cidrs")
        && params["secondary_addr"]
            .as_str()
            .is_none_or(|v| tun.addresses.iter().any(|a| cidr(a, v)))
}
