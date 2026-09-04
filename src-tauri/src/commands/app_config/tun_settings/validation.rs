use std::net::IpAddr;

use crate::errors::{AppError, AppResult};
use crate::models::app_config::AppConfig;
use crate::services::kernel_settings::{
    is_contiguous_mask, network_mask_prefix, next_ip, validate_cidr,
};

pub(super) fn validate(config: &mut AppConfig) -> AppResult<()> {
    let tun = &config.tun;
    let (primary, prefix) = validate_cidr(&tun.addr, "tun.addr")?;
    let mask = tun
        .mask
        .parse::<IpAddr>()
        .map_err(|_| AppError::invalid_argument("tun.mask must be an IP address"))?;
    if primary.is_ipv4() != mask.is_ipv4() {
        return Err(AppError::invalid_argument(
            "TUN address and mask must use the same family",
        ));
    }
    if !is_contiguous_mask(mask) {
        return Err(AppError::invalid_argument("TUN mask must be contiguous"));
    }
    if network_mask_prefix(mask) != Some(prefix) {
        return Err(AppError::invalid_argument(
            "TUN address prefix and mask must describe the same network",
        ));
    }
    let secondary = match tun.secondary_addr.as_deref() {
        Some(value) => {
            let (address, _) = validate_cidr(value, "tun.secondaryAddr")?;
            if !tun.dual_stack || address.is_ipv4() == primary.is_ipv4() {
                return Err(AppError::invalid_argument(
                    "TUN secondary address requires dual-stack and the other address family",
                ));
            }
            Some(address)
        }
        None if tun.dual_stack => Some(if primary.is_ipv4() {
            "fd66::1".parse().unwrap()
        } else {
            "10.66.0.1".parse().unwrap()
        }),
        None => None,
    };
    for cidr in tun.include_cidrs.iter().chain(&tun.exclude_cidrs) {
        let (address, _) = validate_cidr(cidr, "TUN route CIDR")?;
        if !tun.dual_stack && address.is_ipv4() != primary.is_ipv4() {
            return Err(AppError::invalid_argument(
                "TUN route CIDR has no configured address family",
            ));
        }
    }
    if tun.mtu < 576 || tun.tag.trim().is_empty() {
        return Err(AppError::invalid_argument(
            "TUN requires a tag and an MTU between 576 and 65535",
        ));
    }
    if tun.dns_hijack && (!config.dns.enabled || config.dns.config.is_none()) {
        return Err(AppError::invalid_argument(
            "save an enabled DNS configuration before enabling TUN DNS hijack",
        ));
    }
    if config.dns.enabled {
        if let Some(dns) = &config.dns.config {
            let mut owned = vec![primary];
            owned.extend(secondary);
            owned.extend(owned.clone().into_iter().filter_map(next_ip));
            dns.validate_tun_owned_addresses(&owned)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::models::app_config::{AppConfig, AppConfigPatch};
    use crate::services::app_config::prepare_update;

    #[test]
    fn explicit_null_clears_optional_tun_fields_without_changing_desired_state() {
        let mut previous = AppConfig::default();
        previous.tun.enabled = Some(true);
        previous.tun.name = Some("OldTun".into());
        previous.tun.secondary_addr = Some("fd77::1/64".into());
        let patch: AppConfigPatch = serde_json::from_value(serde_json::json!({
            "tun": {"name": null, "secondaryAddr": null, "excludeCidrs": ["203.0.113.10/32"]}
        }))
        .unwrap();
        let mut candidate = prepare_update(&previous, patch).unwrap();
        assert!(candidate.tun.name.is_none());
        assert!(candidate.tun.secondary_addr.is_none());
        assert_eq!(candidate.tun.enabled, Some(true));
        assert_eq!(previous.tun.name.as_deref(), Some("OldTun"));
        super::validate(&mut candidate).unwrap();
    }

    #[test]
    fn rejects_mismatched_tun_prefix_and_network_mask() {
        let mut config = AppConfig::default();
        config.tun.addr = "10.0.0.1/24".into();
        config.tun.mask = "255.255.255.252".into();

        let error = super::validate(&mut config).expect_err("mismatched mask must fail");
        assert!(error.message.contains("same network"));
    }
}
