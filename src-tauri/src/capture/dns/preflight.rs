use crate::errors::{AppError, AppResult};

pub(super) fn explicit_upstreams(dns: &serde_json::Value, peer: &str) -> AppResult<()> {
    let servers = dns
        .get("servers")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| AppError::internal("未取得内核 DNS 上游配置，未修改系统 DNS"))?;
    if servers.is_empty() {
        return Err(AppError::internal("内核没有独立 DNS 上游，未接管系统 DNS"));
    }
    if let Some((name, _)) = servers.iter().find(|(_, server)| {
        server.get("type").and_then(serde_json::Value::as_str) == Some("system")
            || server.get("host").and_then(serde_json::Value::as_str) == Some(peer)
    }) {
        return Err(AppError::internal(format!(
            "DNS 上游「{name}」使用系统解析或指向 TUN 对端；请为它配置独立的 DNS 地址后重试，当前未接管系统 DNS"
        )));
    }
    Ok(())
}

pub(super) fn probe(server: &str) -> AppResult<()> {
    let ip = server
        .parse()
        .map_err(|_| AppError::internal("TUN DNS 对端不是 IP 地址"))?;
    probe_at(std::net::SocketAddr::new(ip, 53))
}

fn probe_at(server: std::net::SocketAddr) -> AppResult<()> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| AppError::internal(e.to_string()))?;
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .map_err(|e| AppError::internal(e.to_string()))?;
    socket
        .connect(server)
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
    if count < 12
        || reply[..2] != id
        || reply[2] & 0x80 == 0
        || reply[2] & 0x7a != 0
        || !matches!(reply[3] & 0x0f, 0 | 3)
    {
        return Err(AppError::internal("TUN DNS 对端返回了无效响应"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
