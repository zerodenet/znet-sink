//! Neutral native Packet conversation leases. No Flow/endpoint state mutation.
use crate::{
    errors::{AppError, AppResult},
    kernel::zero::{commands, queries},
    models::{core::CoreIpcOptions, gui_core::GuiZeroCapabilities, packet_routes::*},
};
use serde_json::{json, Value};
mod ledger;
use ledger::{CloseLedger, LEDGER};

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
    if target.route_id.len() > 1024
        || target.core_instance_id.len() > 1024
        || target.route_id.trim().is_empty()
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
    query: PacketPageQuery,
    client_scope_generation: u64,
) -> AppResult<PacketSnapshot> {
    snapshot_with_ledger(profile_id, options, query, client_scope_generation, &LEDGER).await
}
fn validate_page(query: &PacketPageQuery) -> AppResult<()> {
    if !(1..=500).contains(&query.limit) {
        return Err(AppError::invalid_argument("Packet 每页数量须为1–500"));
    }
    if query.offset > 0 && query.expected.is_none() {
        return Err(AppError::invalid_argument(
            "后续页必须绑定原配置、实例和客户端代次",
        ));
    }
    Ok(())
}
async fn snapshot_with_ledger(
    profile_id: Option<String>,
    options: CoreIpcOptions,
    query: PacketPageQuery,
    client_scope_generation: u64,
    ledger: &CloseLedger,
) -> AppResult<PacketSnapshot> {
    validate_page(&query)?;
    let (blocked_targets, close_ledger_full) = ledger.snapshot()?;
    let caps = queries::zero_capabilities(Some(options.clone())).await?;
    let reason = support_reason(&caps);
    let close_reason = close_reason(&caps);
    if reason.is_some() {
        return Ok(PacketSnapshot {
            offset: query.offset,
            limit: query.limit,
            client_scope_generation,
            blocked_targets,
            close_ledger_full,
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
    if query.expected.as_ref().is_some_and(|expected| {
        expected.profile_id != profile_id
            || expected.client_scope_generation != client_scope_generation
            || expected.core_instance_id != before.core_instance_id
            || expected.config_revision != before.config_revision.to_string()
    }) {
        return Err(AppError::conflict(
            "packet_page",
            "",
            "分页所属配置、实例或代次已变化，请返回第一页重新检查",
        ));
    }
    // Bounded view, not an unlimited historical store. The current API lists
    // active/closing leases, and expired leases can disappear during a read.
    let page: PageWire = decode(
        queries::query_value(
            json!(zero_api::QueryRequest::PacketRoutes(
                zero_api::PacketRouteListQuery {
                    offset: query.offset as usize,
                    limit: Some(query.limit as usize),
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
    let end = (query.offset as usize)
        .checked_add(page.routes.len())
        .ok_or_else(|| AppError::internal("Packet 分页位置溢出"))?;
    let expected_next = (end < page.total).then_some(end);
    if page.routes.len() > query.limit as usize
        || page.routes.len() > page.total
        || (!page.routes.is_empty() && ((query.offset as usize) >= page.total || end > page.total))
        || (end < page.total && page.routes.is_empty())
        || page.next_offset != expected_next
        || page.total > u32::MAX as usize
        || page.next_offset.is_some_and(|n| n > u32::MAX as usize)
    {
        return Err(AppError::internal(
            "Packet 路径分页响应无效或超出客户端可表示范围",
        ));
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
    for row in routes.iter().filter(|row| row.state == "closed") {
        ledger.ended(&PacketTarget {
            profile_id: profile_id.clone(),
            client_scope_generation,
            route_id: row.route_id.clone(),
            core_instance_id: row.core_instance_id.clone(),
            config_revision: row.config_revision.clone(),
        })?;
    }
    let (blocked_targets, close_ledger_full) = ledger.snapshot()?;
    Ok(PacketSnapshot {
        offset: query.offset,
        limit: query.limit,
        client_scope_generation,
        blocked_targets,
        close_ledger_full,
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
    detail_with_ledger(target, options, &LEDGER).await
}
async fn detail_with_ledger(
    target: PacketTarget,
    options: CoreIpcOptions,
    ledger: &CloseLedger,
) -> AppResult<PacketRoute> {
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
    let current = get(&target.route_id, options.clone()).await;
    let after = queries::runtime_identity(Some(options)).await?;
    if before != after || after.core_instance_id != target.core_instance_id {
        return Err(AppError::conflict(
            "packet_route",
            &target.route_id,
            "内核实例已变化，请刷新",
        ));
    }
    match current {
        Ok(current) => {
            ensure(&current, &target)?;
            if current.state == "closed" {
                ledger.ended(&target)?;
            }
            Ok(current)
        }
        Err(error) if error.code == "not_found" => {
            ledger.ended(&target)?;
            Err(AppError {
                code: "not_found",
                message: "同一内核实例已确认原 Packet 会话不再存在".into(),
                details: Some(json!({"packetTargetEnded":PacketPhysicalTarget::from(&target)})),
            })
        }
        Err(error) => Err(error),
    }
}
pub async fn close(target: PacketTarget, options: CoreIpcOptions) -> AppResult<PacketCloseResult> {
    close_with_ledger(target, options, &LEDGER).await
}
async fn close_with_ledger(
    target: PacketTarget,
    options: CoreIpcOptions,
    ledger: &CloseLedger,
) -> AppResult<PacketCloseResult> {
    if ledger.contains(&target)? {
        return Err(AppError::conflict(
            "packet_close",
            "",
            "原物理会话关闭仍在途或结果未确认，请只读核对原目标",
        ));
    }
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
    ledger.reserve(&target)?;
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
                let error = AppError::core_response(v);
                if explicit_rejection(&error) {
                    ledger.ended(&target)?;
                    return Err(error);
                }
                return Ok(unconfirmed("关闭回执未提供兼容的明确拒绝，保留原目标屏障"));
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
        Err(error) if explicit_rejection(&error) => {
            ledger.ended(&target)?;
            return Err(error);
        }
        Err(_) => {
            return Ok(unconfirmed(
                "关闭结果未确认；回执缺失、不兼容或读取中断，保留原目标屏障且不重放",
            ))
        }
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
    let current = get(&target.route_id, options.clone()).await;
    let final_identity = queries::runtime_identity(Some(options)).await;
    if !final_identity
        .as_ref()
        .is_ok_and(|after| after == &identity)
    {
        return Ok(unconfirmed(
            "核对原会话时内核身份发生变化或读取失败，请保留原关闭屏障",
        ));
    }
    let result: AppResult<PacketCloseResult> = match current {
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
    };
    if result
        .as_ref()
        .is_ok_and(|result| result.status == "confirmed")
    {
        ledger.ended(&target)?;
    }
    result
}
// Only typed, correlated, negative core receipts prove this reservation did
// not start a mutation. Unknown transport/decoder/core errors stay fenced.
fn explicit_rejection(error: &AppError) -> bool {
    [
        "permission_denied",
        "invalid_argument",
        "feature_disabled",
        "conflict",
        "unsupported",
        "not_found",
    ]
    .contains(&error.code)
        && error.details.as_ref().is_some_and(|receipt| {
            receipt["ok"].as_bool() == Some(false) || receipt["accepted"].as_bool() == Some(false)
        })
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
    fn invalid_response_and_generic_errors_are_not_treated_as_safe_rejections() {
        for code in [
            "invalid_response",
            "timeout",
            "connection_closed",
            "core_error",
            "internal",
            "io_error",
        ] {
            let error = AppError {
                code,
                message: "fixture".into(),
                details: Some(json!({"ok":false})),
            };
            assert!(!explicit_rejection(&error));
        }
        assert!(!explicit_rejection(&AppError {
            code: "permission_denied",
            message: "fixture".into(),
            details: None
        }));
        assert!(explicit_rejection(&AppError::core_response(
            json!({"ok":false,"error":{"code":"permission_denied","message":"fixture"}})
        )));
    }
    #[test]
    fn rejects_invalid_identity_and_revision() {
        let target = PacketTarget {
            profile_id: None,
            client_scope_generation: 1,
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
