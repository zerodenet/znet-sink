use crate::errors::{AppError, AppResult};

pub(super) fn effective_dns(
    app: &crate::models::app_config::AppConfig,
    source: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let mut config = source.clone();
    if app.overrides.dns || config.pointer("/runtime/dns").is_none() {
        crate::configuration::dns::apply_global_dns(&mut config, &app.dns)?;
    }
    config
        .pointer("/runtime/dns")
        .cloned()
        .ok_or_else(|| AppError::internal("当前内核配置未启用独立 DNS，未修改系统解析入口"))
}

pub(super) fn explicit_upstreams(dns: &serde_json::Value, peer: &str) -> AppResult<()> {
    let servers = dns
        .get("servers")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| AppError::internal("未取得内核 DNS 上游配置，未修改系统 DNS"))?;
    if servers.is_empty()
        || servers.values().any(|server| {
            server.get("type").and_then(serde_json::Value::as_str) == Some("system")
                || server.get("host").and_then(serde_json::Value::as_str) == Some(peer)
        })
    {
        return Err(AppError::internal(
            "请先配置独立的 DNS 上游；系统 DNS 接管不能将内核上游再次指向 TUN",
        ));
    }
    Ok(())
}

pub(super) fn probe(server: &str) -> AppResult<()> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| AppError::internal(e.to_string()))?;
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .map_err(|e| AppError::internal(e.to_string()))?;
    socket
        .connect((server, 53))
        .map_err(|e| AppError::internal(e.to_string()))?;
    let id = (crate::services::common::now_unix_ms() as u16).to_be_bytes();
    let mut request = vec![id[0], id[1], 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in ["znet-sink-dns-probe", "invalid"] {
        request.push(label.len() as u8);
        request.extend_from_slice(label.as_bytes());
    }
    request.extend_from_slice(&[0, 0, 1, 0, 1]);
    socket
        .send(&request)
        .map_err(|e| AppError::internal(e.to_string()))?;
    let mut reply = [0u8; 4096];
    let count = socket
        .recv(&mut reply)
        .map_err(|e| AppError::internal(format!("TUN DNS 对端不可达：{e}")))?;
    if count < 12 || reply[..2] != id || reply[2] & 0x80 == 0 {
        return Err(AppError::internal("TUN DNS 对端返回了无效响应"));
    }
    Ok(())
}
