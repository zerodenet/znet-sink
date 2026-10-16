//! One authenticated, pinned read; future events never enlarge the export payload.
use super::*;
use crate::{
    kernel::{
        configuration::BoundControl,
        zero::{parsing, queries::KernelRuntimeIdentity},
    },
    models::core::CoreIpcOptions,
};
#[derive(Clone)]
pub(super) struct Lease {
    pub control: BoundControl,
    pub identity: KernelRuntimeIdentity,
    pub binding: String,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Filter {
    pub search: Option<String>,
    pub protocol: Option<String>,
    pub outbound: Option<String>,
}
impl Filter {
    pub fn validate(&self) -> AppResult<()> {
        if self.search.as_ref().is_some_and(|s| s.len() > 2048)
            || self.outbound.as_ref().is_some_and(|s| s.len() > 2048)
            || self
                .protocol
                .as_ref()
                .is_some_and(|p| !matches!(p.as_str(), "tcp" | "udp"))
        {
            return Err(AppError::invalid_argument(
                "活动连接筛选格式或长度不符合当前协议",
            ));
        }
        Ok(())
    }
}
fn compatible(raw: &Value) -> AppResult<()> {
    let caps = parsing::parse_capabilities(raw, None);
    let contracts = caps.contracts.as_ref().is_some_and(|c| {
        c.capabilities.minimum_supported == 1
            && c.capabilities.current >= 1
            && c.control_api.minimum_supported == 1
            && c.control_api.current >= 1
    });
    if !caps.available
        || !contracts
        || !["runtime_snapshot", "flow_snapshot"]
            .iter()
            .all(|feature| caps.features.iter().any(|f| f == feature))
    {
        return Err(AppError {
            code: "unsupported",
            message: "当前内核未声明兼容的活动连接观测与实例契约".into(),
            details: None,
        });
    }
    // QueryRequest::required_permission is Read. The pinned local IPC server
    // authorizes every actual query; capabilities.permissions is NOT a grant.
    Ok(())
}
fn read_error(error: AppError) -> AppError {
    let code = error
        .details
        .as_ref()
        .and_then(|d| d.pointer("/error/code"))
        .and_then(Value::as_str)
        .unwrap_or(error.code);
    let code = match code {
        "permission_denied" => "permission_denied",
        "unsupported" => "unsupported",
        "conflict" => "conflict",
        _ => "core_unavailable",
    };
    AppError {
        code,
        message: if code == "permission_denied" {
            "内核拒绝读取活动连接：当前调用没有读取权限"
        } else {
            "内核活动连接读取未完成，请重新检查运行服务、权限与控制通道"
        }
        .into(),
        details: None,
    }
}
async fn capabilities(control: &BoundControl) -> AppResult<()> {
    let raw = control
        .call(json!({"type":"query","request":{"capabilities":{}}}))
        .await
        .map_err(read_error)?;
    compatible(raw.get("capabilities").unwrap_or(&raw))
}
pub(super) async fn verify(lease: &Lease) -> AppResult<()> {
    capabilities(&lease.control).await?;
    if lease.control.identity().await.map_err(read_error)? != lease.identity {
        return Err(conflict("内核实例或配置已变化，请重新预览活动连接"));
    }
    Ok(())
}
fn matches(flow: &crate::models::gui_core::GuiConnection, filter: &Filter) -> bool {
    if filter
        .protocol
        .as_deref()
        .is_some_and(|p| p != flow.network)
        || filter
            .outbound
            .as_deref()
            .is_some_and(|out| Some(out) != flow.outbound_tag.as_deref())
    {
        return false;
    }
    let query = filter.search.as_deref().unwrap_or("").trim().to_lowercase();
    query.is_empty()
        || [
            Some(flow.destination.as_str()),
            flow.source.as_deref(),
            Some(flow.flow_id.as_str()),
            flow.policy_tag.as_deref(),
            flow.outbound_tag.as_deref(),
            flow.process_name.as_deref(),
            flow.process_path.as_deref(),
            flow.matched_rule.as_deref(),
            flow.remote_destination.as_deref(),
        ]
        .into_iter()
        .chain(flow.selection_chain.iter().map(|s| Some(s.as_str())))
        .chain(flow.relay_chain.iter().map(|s| Some(s.as_str())))
        .flatten()
        .any(|text| text.to_lowercase().contains(&query))
}
pub(super) async fn read(
    control: BoundControl,
    binding: String,
    filter: &Filter,
) -> AppResult<(Lease, Vec<Value>, bool, u64)> {
    filter.validate()?;
    capabilities(&control).await?;
    let identity = control.identity().await.map_err(read_error)?;
    let query = zero_api::QueryRequest::ActiveFlows(zero_api::FlowListQuery {
        limit: Some(RECORDS + 1),
        filter: zero_api::FlowFilter {
            network: filter.protocol.as_deref().map(|p| {
                if p == "tcp" {
                    zero_api::Network::Tcp
                } else {
                    zero_api::Network::Udp
                }
            }),
            ..Default::default()
        },
    });
    let response = control
        .call(json!({"type":"query","request":query}))
        .await
        .map_err(read_error)?;
    let raw = response
        .get("active_flows")
        .ok_or_else(|| AppError::internal("活动连接响应缺少公共查询类型"))?;
    let rows = raw
        .as_array()
        .ok_or_else(|| AppError::internal("活动连接快照格式不兼容"))?;
    if rows.len() > RECORDS + 1 {
        return Err(AppError::invalid_argument("活动连接响应超出有界读取预算"));
    }
    let truncated = rows.len() > RECORDS;
    let mut ids = std::collections::HashSet::new();
    let mut records = Vec::new();
    for raw in rows.iter().take(RECORDS) {
        let snapshot: zero_api::FlowSnapshot = serde_json::from_value(raw.clone())
            .map_err(|_| AppError::internal("活动连接快照不符合公共schema"))?;
        let record = snapshot.record.ok_or_else(|| AppError {
            code: "unsupported",
            message: "当前内核没有完整活动连接记录，不能以缺失字段的默认值导出".into(),
            details: None,
        })?;
        if record.flow_id.is_empty()
            || !ids.insert(record.flow_id.clone())
            || record.state == zero_api::FlowState::Completed
        {
            return Err(AppError::internal("活动连接身份重复、缺失或状态不兼容"));
        }
        let value = serde_json::to_value(&record)
            .map_err(|_| AppError::internal("活动连接记录映射失败"))?;
        let flow = parsing::parse_connection(&value)
            .ok_or_else(|| AppError::internal("活动连接记录映射缺少身份"))?;
        if matches(&flow, filter) {
            let mut public = project_fields(
                &serde_json::to_value(&flow).map_err(|_| AppError::internal("活动连接投影失败"))?,
            );
            public["source"] = json!("authoritative_active_flow_snapshot");
            public["state"] = json!(record.state);
            records.push(public);
        }
    }
    let observed = now_unix_ms();
    if control.identity().await.map_err(read_error)? != identity {
        return Err(conflict("读取期间内核实例或配置已变化，请重新预览"));
    }
    Ok((
        Lease {
            control,
            identity,
            binding,
        },
        records,
        truncated,
        observed,
    ))
}
pub(super) fn runtime_available(native: bool) -> AppResult<()> {
    if native {
        return Err(crate::mobile_host::unavailable(
            "native_active_connection_export_preview",
        ));
    }
    Ok(())
}
pub(super) fn options(state: &AppState) -> AppResult<(CoreIpcOptions, String)> {
    let core = lock(state.app_config(), "app_config")?.core.clone();
    if core.kernel != "zero" {
        return Err(AppError {
            code: "unsupported",
            message: "当前内核没有活动连接导出适配器".into(),
            details: None,
        });
    }
    let binding =
        digest(serde_json::to_vec(&core).map_err(|_| AppError::internal("内核绑定读取失败"))?);
    Ok((
        crate::services::core_config::ipc_options_from_app_config(&core),
        binding,
    ))
}
pub(crate) async fn preview(
    state: &AppState,
    filter: Filter,
) -> AppResult<ConnectionExportPreview> {
    runtime_available(cfg!(any(target_os = "android", target_os = "ios")))?;
    crate::services::interaction_mode::require_pro_mode(state, "active_connection_export")?;
    let _guard = state.proxy_config_operation().lock().await;
    let scope = state.client_core_snapshot().scope;
    let (ipc_options, binding) = options(state)?;
    let (control, records, truncated, observed) = read(
        BoundControl::connect(ipc_options)
            .await
            .map_err(read_error)?,
        binding.clone(),
        &filter,
    )
    .await?;
    state.ensure_client_scope(Some(&scope))?;
    if options(state)?.1 != binding {
        return Err(conflict("内核控制绑定已变化，请重新预览"));
    }
    let now = now_unix_ms();
    let (view, bytes) = freeze(&filter, &control, records, truncated, observed, now)?;
    let mut plans = lock(&PLANS, "connection_exports")?;
    plans.retain(|_, p| p.expires > now);
    if plans.len() >= 4 {
        return Err(AppError::invalid_argument(
            "连接导出预览已达容量，请等待旧预览过期",
        ));
    }
    plans.insert(
        view.plan_id.clone(),
        Plan {
            active: Some(control),
            scope,
            expires: now.saturating_add(TTL),
            bytes: Some(bytes),
        },
    );
    Ok(view)
}

pub(super) fn freeze(
    filter: &Filter,
    lease: &Lease,
    records: Vec<Value>,
    truncated: bool,
    observed: u64,
    now: u64,
) -> AppResult<(ConnectionExportPreview, Vec<u8>)> {
    if records.len() > RECORDS {
        return Err(AppError::invalid_argument("活动连接快照超过2000条预算"));
    }
    let fingerprint = digest(
        serde_json::to_vec(&filter).map_err(|_| AppError::invalid_argument("活动连接筛选无效"))?,
    );
    let omissions=vec!["活动连接为当前内核一次真实查询的固定快照，随后新增或结束的连接不改变导出材料；不是历史报表。","保留校验后的IP、套接字、域名、连接ID与真实计量；凭据、URL、认证材料、进程及自由文本错误排除。"];
    let value = json!({"schemaVersion":"znet.redacted-active-connections.v1","createdAtUnixMs":now.to_string(),"observedAtUnixMs":observed.to_string(),"coreInstanceId":lease.identity.core_instance_id,"configRevision":lease.identity.config_revision.to_string(),"filtersFingerprint":fingerprint,"recordLimit":RECORDS,"hasMore":truncated,"source":"authoritative_active_flow_snapshot","omissions":omissions,"records":records});
    let bytes =
        serde_json::to_vec(&value).map_err(|_| AppError::internal("活动连接快照编码失败"))?;
    if bytes.len() > BYTES {
        return Err(AppError::invalid_argument(
            "活动连接快照超过2MiB预算，请缩小筛选范围",
        ));
    }
    let view = ConnectionExportPreview {
        source: "active",
        core_instance_id: Some(lease.identity.core_instance_id.clone()),
        config_revision: Some(lease.identity.config_revision.to_string()),
        observed_at_unix_ms: Some(observed.to_string()),
        plan_id: format!(
            "active-export-{now}-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ),
        records: records.len(),
        truncated,
        bytes: bytes.len() as u32,
        expires_at_unix_ms: now.saturating_add(TTL).to_string(),
        filters_fingerprint: fingerprint,
        omissions,
    };
    Ok((view, bytes))
}
