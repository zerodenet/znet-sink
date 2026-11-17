//! A failed packet probe is a definite failure. An unavailable IPC observation
//! is Unknown and must never authorize a host DNS mutation.
use super::*;
use std::future::Future;

#[derive(Debug)]
pub(super) enum Failure {
    Unknown(AppError),
    Confirmed(AppError),
}

pub(super) async fn verify<P, Q, F>(
    before: &GuiTunStatus,
    probe: P,
    query: Q,
) -> Result<(), Failure>
where
    P: Future<Output = AppResult<()>>,
    Q: FnOnce() -> F,
    F: Future<Output = AppResult<GuiTunStatus>>,
{
    probe.await.map_err(Failure::Confirmed)?;
    let after = query().await.map_err(Failure::Unknown)?;
    if after.name != before.name
        || !after.enabled
        || !after.healthy
        || after.dns_hijacked_queries <= before.dns_hijacked_queries
    {
        return Err(Failure::Confirmed(AppError::internal(
            "DNS 探测未被当前 TUN 内核确认拦截，已停止本应用的系统 DNS 接管",
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
