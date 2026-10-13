//! Neutral native Packet conversation leases. No Flow/endpoint state mutation.
use crate::{
    errors::{AppError, AppResult},
    kernel::zero::{commands, queries},
    models::{core::CoreIpcOptions, gui_core::GuiZeroCapabilities, packet_routes::*},
};
use serde_json::{json, Value};

use zero_api::{
    PacketRouteListSnapshot as PageWire, PacketRouteSnapshot as RouteWire, PacketRouteState,
};

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> AppResult<T> {
    serde_json::from_value(value)
        .map_err(|_| AppError::internal("Packet 路径响应格式不兼容，请更新内核或重新检查"))
}
fn route(w: RouteWire) -> AppResult<PacketRoute> {
    if w.route_id.trim().is_empty() || w.core_instance_id.trim().is_empty() {
        return Err(AppError::internal("Packet 路径缺少有效身份或状态"));
    }
    Ok(PacketRoute {
        route_id: w.route_id,
        core_instance_id: w.core_instance_id,
        config_revision: w.config_revision.to_string(),
        inbound_tag: w.inbound_tag,
        outbound_tag: w.outbound_tag,
        endpoints: w
            .endpoints
            .into_iter()
            .map(|e| PacketEndpoint {
                endpoint_id: e.endpoint_id,
                generation: e.generation.map(|g| g.to_string()),
            })
            .collect(),
        source: w.source,
        destination: w.destination,
        ip_protocol: w.ip_protocol,
        translated: w.translated,
        started_at_unix_ms: w.started_at_unix_ms.to_string(),
        state: match w.state {
            PacketRouteState::Active => "active",
            PacketRouteState::Closing => "closing",
            PacketRouteState::Closed => "closed",
        }
        .into(),
    })
}
fn support_reason(caps: &GuiZeroCapabilities) -> Option<String> {
    if !caps.available {
        return Some("内核能力未确认，请重新检查".into());
    }
    if !caps.contracts.as_ref().is_some_and(|c| {
        c.control_api.minimum_supported == 1
            && c.control_api.current >= 1
            && c.capabilities.minimum_supported == 1
            && c.capabilities.current >= 1
    }) {
        return Some("当前内核未声明兼容的控制面契约".into());
    }
    if !caps
        .features
        .iter()
        .any(|f| f == "packet_route_management_v1")
    {
        return Some("当前内核未声明 Packet 路径管理能力（需要 raw-ip-runtime）".into());
    }

    None
}
fn close_reason(caps: &GuiZeroCapabilities) -> Option<String> {
    // This service uses the existing OS-authenticated local IPC transport,
    // whose server assigns ipc-local Admin. caps.permissions describes
    // adapter metadata, not the authenticated caller's grants. Every command
    // remains subject to the core's required_permission check.
    support_reason(caps).or_else(|| {
        cfg!(any(target_os = "android", target_os = "ios"))
            .then(|| "平台服务控制桥尚未验收，Packet 关闭暂不可用".into())
    })
}

fn revision(target: &PacketTarget) -> AppResult<u64> {
    if target.route_id.trim().is_empty()
        || target.core_instance_id.trim().is_empty()
        || target.config_revision.is_empty()
        || !target.config_revision.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(AppError::invalid_argument("Packet 会话目标身份无效"));
    }
    target
        .config_revision
        .parse()
        .map_err(|_| AppError::invalid_argument("Packet 配置版本超出 u64 范围"))
}
fn ensure(route: &PacketRoute, target: &PacketTarget) -> AppResult<()> {
    if route.route_id != target.route_id
        || route.core_instance_id != target.core_instance_id
        || route.config_revision != target.config_revision
    {
        return Err(AppError::conflict(
            "packet_route",
            &target.route_id,
            "会话或配置已变化，请刷新后重新选择",
        ));
    }
    Ok(())
}
pub async fn snapshot(
    profile_id: Option<String>,
    options: CoreIpcOptions,
) -> AppResult<PacketSnapshot> {
    let caps = queries::zero_capabilities(Some(options.clone())).await?;
    let reason = support_reason(&caps);
    let close_reason = close_reason(&caps);
    if reason.is_some() {
        return Ok(PacketSnapshot {
            supported: false,
            can_close: false,
            reason,
            close_reason,
            profile_id,
            core_instance_id: None,
            config_revision: None,
            observed_at_unix_ms: None,
            routes: vec![],
            total: None,
            next_offset: None,
        });
    }
    let before = queries::runtime_identity(Some(options.clone())).await?;
    // Bounded view, not an unlimited historical store. The current API lists
    // active/closing leases, and expired leases can disappear during a read.
    let page: PageWire = decode(
        queries::query_value(
            json!(zero_api::QueryRequest::PacketRoutes(
                zero_api::PacketRouteListQuery {
                    limit: Some(500),
                    ..Default::default()
                }
            )),
            "packet_routes",
            Some(options.clone()),
        )
        .await?,
    )?;
    let after = queries::runtime_identity(Some(options)).await?;
    if before != after
        || page.core_instance_id != after.core_instance_id
        || page.config_revision != after.config_revision
    {
        return Err(AppError::conflict(
            "packet_routes",
            "",
            "内核实例或配置已变化，请重新检查",
        ));
    }
    if page.routes.len() > 500
        || page.routes.len() > page.total
        || page
            .next_offset
            .is_some_and(|n| n != page.routes.len() || n >= page.total)
    {
        return Err(AppError::internal("Packet 路径分页响应无效"));
    }
    let mut ids = std::collections::HashSet::new();
    let mut routes = Vec::new();
    for row in page.routes {
        let row = route(row)?;
        if row.core_instance_id != after.core_instance_id || !ids.insert(row.route_id.clone()) {
            return Err(AppError::internal("Packet 路径实例或会话身份不一致"));
        }
        // Existing leases may retain their creation revision after a hot reload.
        // Such rows remain observable but cannot be closed with an old revision.
        routes.push(row);
    }
    Ok(PacketSnapshot {
        supported: true,
        can_close: close_reason.is_none(),
        reason: None,
        close_reason,
        profile_id,
        core_instance_id: Some(after.core_instance_id),
        config_revision: Some(after.config_revision.to_string()),
        observed_at_unix_ms: Some(page.observed_at_unix_ms.to_string()),
        routes,
        total: Some(page.total),
        next_offset: page.next_offset,
    })
}
async fn get(id: &str, options: CoreIpcOptions) -> AppResult<PacketRoute> {
    route(decode(
        queries::query_value(
            json!(zero_api::QueryRequest::PacketRoute(
                zero_api::PacketRouteGetQuery {
                    route_id: id.into()
                }
            )),
            "packet_route",
            Some(options),
        )
        .await?,
    )?)
}
pub async fn detail(target: PacketTarget, options: CoreIpcOptions) -> AppResult<PacketRoute> {
    revision(&target)?;
    let caps = queries::zero_capabilities(Some(options.clone())).await?;
    if let Some(reason) = support_reason(&caps) {
        return Err(AppError {
            code: "unsupported",
            message: reason,
            details: None,
        });
    }
    let before = queries::runtime_identity(Some(options.clone())).await?;
    let current = get(&target.route_id, options.clone()).await?;
    ensure(&current, &target)?;
    let after = queries::runtime_identity(Some(options)).await?;
    if before != after || after.core_instance_id != target.core_instance_id {
        return Err(AppError::conflict(
            "packet_route",
            &target.route_id,
            "内核实例已变化，请刷新",
        ));
    }
    Ok(current)
}
pub async fn close(target: PacketTarget, options: CoreIpcOptions) -> AppResult<PacketCloseResult> {
    let expected_revision = revision(&target)?;
    let caps = queries::zero_capabilities(Some(options.clone())).await?;
    if let Some(reason) = close_reason(&caps) {
        return Err(AppError {
            code: "permission_denied",
            message: reason,
            details: None,
        });
    }
    let current = get(&target.route_id, options.clone()).await?;
    ensure(&current, &target)?;
    if current.state != "active" {
        return Err(AppError::conflict(
            "packet_route",
            &target.route_id,
            "会话正在关闭或已结束，请重新检查",
        ));
    }
    let response = commands::run_command(
        "packet_routes.close",
        json!(zero_api::PacketRouteCloseCommand {
            route_id: target.route_id.clone(),
            expected_core_instance_id: target.core_instance_id.clone(),
            expected_config_revision: Some(expected_revision)
        }),
        Some(options.clone()),
    )
    .await;
    let receipt = match response {
        Ok(v) => {
            if v["accepted"].as_bool() == Some(false) {
                return Err(AppError::core_response(v));
            }
            if v["accepted"].as_bool() != Some(true) {
                return Ok(unconfirmed("内核未提供明确关闭回执；请重新检查原会话"));
            }
            let result = &v["result"];
            let receipt = match decode(result["route"].clone()).and_then(route) {
                Ok(receipt) if ensure(&receipt, &target).is_ok() => receipt,
                _ => return Ok(unconfirmed("关闭回执身份或格式不兼容，请重新检查原会话")),
            };
            (result["closed"].as_bool() == Some(true) && receipt.state == "closed")
                .then_some(receipt)
        }
        Err(e)
            if [
                "timeout",
                "connection_closed",
                "core_unavailable",
                "io_error",
                "internal",
            ]
            .contains(&e.code) =>
        {
            return Ok(unconfirmed(
                "关闭结果未确认；请求可能仍在执行，请重新检查原会话，勿重复提交",
            ))
        }
        Err(e) => return Err(e),
    };
    let identity = match queries::runtime_identity(Some(options.clone())).await {
        Ok(i) => i,
        Err(_) => {
            return Ok(unconfirmed(
                "关闭回执已收到，但内核状态读取失败；请重新检查",
            ))
        }
    };
    if identity.core_instance_id != target.core_instance_id
        || identity.config_revision != expected_revision
    {
        return Ok(unconfirmed("内核或配置已变化，原关闭结果仅供历史核对"));
    }
    match get(&target.route_id, options).await {
        Ok(r) if ensure(&r, &target).is_ok() && r.state == "closed" => Ok(PacketCloseResult {
            status: "confirmed".into(),
            message: "原 Packet 会话已结束；端点启用意图保持不变".into(),
            route: Some(r),
        }),
        Err(e) if e.code == "not_found" && receipt.is_some() => Ok(PacketCloseResult {
            status: "confirmed".into(),
            message: "内核已确认释放原 Packet 会话".into(),
            route: receipt,
        }),
        _ => Ok(unconfirmed("原会话尚未确认释放，请重新检查其状态")),
    }
}
fn unconfirmed(message: &str) -> PacketCloseResult {
    PacketCloseResult {
        status: "unconfirmed".into(),
        message: message.into(),
        route: None,
    }
}

#[cfg(all(test, unix))]
mod ipc_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn u64_projection_is_lossless() {
        let row: RouteWire = decode(json!({"route_id":"r","core_instance_id":"i","config_revision":u64::MAX,"inbound_tag":"in","outbound_tag":"out","endpoints":[{"endpoint_id":"e","generation":u64::MAX}],"source":"::1","destination":"::2","ip_protocol":58,"translated":false,"started_at_unix_ms":u64::MAX,"state":"active"})).unwrap();
        let row = route(row).unwrap();
        assert_eq!(row.config_revision, u64::MAX.to_string());
        assert_eq!(row.endpoints[0].generation, Some(u64::MAX.to_string()));
    }
    #[test]
    fn rejects_invalid_identity_and_revision() {
        let target = PacketTarget {
            profile_id: None,
            route_id: "r".into(),
            core_instance_id: "i".into(),
            config_revision: "18446744073709551615".into(),
        };
        assert_eq!(revision(&target).unwrap(), u64::MAX);
        let mut invalid = target.clone();
        invalid.config_revision = "18446744073709551616".into();
        assert!(revision(&invalid).is_err());
        invalid.config_revision = "1e2".into();
        assert!(revision(&invalid).is_err());
        invalid = target.clone();
        invalid.core_instance_id.clear();
        assert!(revision(&invalid).is_err());
    }
}
