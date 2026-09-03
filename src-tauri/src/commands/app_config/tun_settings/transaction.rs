use std::future::Future;
use std::time::Duration;

use crate::errors::{AppError, AppResult};
use crate::kernel::zero::queries::KernelRuntimeIdentity;
use crate::models::app_config::{AppConfig, AppTunConfig};
use crate::models::zero_runtime::GuiTunStatus;

pub(super) struct Snapshot {
    pub identity: KernelRuntimeIdentity,
    pub status: GuiTunStatus,
}

pub(super) trait Backend: Sync {
    fn snapshot(&self) -> impl Future<Output = AppResult<Snapshot>> + Send;
    fn stop(&self) -> impl Future<Output = AppResult<()>> + Send;
    fn start(&self, tun: &AppTunConfig) -> impl Future<Output = AppResult<()>> + Send;
    fn persist(&self, config: &AppConfig) -> AppResult<()>;
}

fn conflict(message: &str) -> AppError {
    AppError::conflict("tun", "runtime", message)
}

pub(super) fn matches(status: &GuiTunStatus, tun: &AppTunConfig) -> bool {
    status.enabled
        && status.healthy
        && !status.managed_by_config
        && status.addr.as_deref() == Some(tun.addr.as_str())
        && status.mtu == Some(tun.mtu)
        && status.tag.as_deref() == Some(tun.tag.as_str())
        && tun
            .name
            .as_ref()
            .is_none_or(|name| status.name.as_ref() == Some(name))
        && status.auto_route
        && status.strict_route
        && status.dual_stack == tun.dual_stack
        && status.dns_hijack == tun.dns_hijack
        && status.include_cidrs.as_ref() == Some(&tun.include_cidrs)
        && status.exclude_cidrs.as_ref() == Some(&tun.exclude_cidrs)
        && tun
            .secondary_addr
            .as_ref()
            .is_none_or(|addr| status.addresses.contains(addr))
}

async fn owned(backend: &impl Backend, identity: &KernelRuntimeIdentity) -> AppResult<Snapshot> {
    let snapshot = backend.snapshot().await?;
    if snapshot.identity != *identity || snapshot.status.managed_by_config {
        return Err(conflict(
            "core instance, config revision or TUN ownership changed; refresh settings",
        ));
    }
    Ok(snapshot)
}

fn transient(error: &AppError) -> bool {
    matches!(
        error.code,
        "timeout" | "connection_closed" | "core_unavailable"
    ) || error.message.contains("IPC request timed out")
        || error.message.contains("IPC connection closed")
}

// A timed-out command may still be executing. Observe its result, never resend
// the mutation or start a competing rollback while completion remains unknown.
async fn transition(
    backend: &impl Backend,
    identity: &KernelRuntimeIdentity,
    expected: Option<&AppTunConfig>,
    desired: Option<&AppTunConfig>,
) -> AppResult<()> {
    let current = owned(backend, identity).await?;
    if !expected.map_or(!current.status.enabled, |tun| matches(&current.status, tun)) {
        return Err(conflict(
            "TUN parameters changed before applying; refresh settings",
        ));
    }
    let result = match desired {
        Some(tun) => backend.start(tun).await,
        None => backend.stop().await,
    };
    if let Err(error) = &result {
        if !transient(error) {
            return Err(error.clone());
        }
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        match owned(backend, identity).await {
            Ok(snapshot)
                if desired.map_or(!snapshot.status.enabled, |tun| {
                    matches(&snapshot.status, tun)
                }) =>
            {
                return Ok(())
            }
            Err(error) if error.code == "conflict" => return Err(error),
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(AppError {
                code: "timeout",
                message: "TUN transition could not be verified; refresh its status before retrying"
                    .into(),
                details: Some(serde_json::json!({"tunTransitionUncertain": true})),
            });
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

pub(super) async fn apply(
    backend: &impl Backend,
    previous: &AppConfig,
    candidate: &AppConfig,
    inspect_runtime: bool,
) -> AppResult<()> {
    if !inspect_runtime || previous.tun == candidate.tun {
        return backend.persist(candidate);
    }
    let before = backend.snapshot().await?;
    if before.status.managed_by_config {
        return backend.persist(candidate); // Local defaults do not override profile-owned TUN.
    }
    if !before.status.enabled {
        return backend.persist(candidate); // Saving settings must not start an inactive TUN.
    }
    if previous.tun.enabled == Some(false) || !matches(&before.status, &previous.tun) {
        return Err(conflict("running TUN does not match the saved client settings; refresh or restart TUN before applying"));
    }
    let result = async {
        transition(backend, &before.identity, Some(&previous.tun), None).await?;
        transition(backend, &before.identity, None, Some(&candidate.tun)).await?;
        // Persist only after the actual running parameters have been verified.
        backend.persist(candidate)
    }
    .await;
    if let Err(mut error) = result {
        let uncertain = error
            .details
            .as_ref()
            .and_then(|v| v.get("tunTransitionUncertain"))
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        let recovery = if uncertain || error.code == "conflict" {
            Err(conflict(
                "rollback skipped because runtime completion or ownership is uncertain",
            ))
        } else {
            rollback(backend, &before.identity, previous, candidate).await
        };
        let restored = recovery.is_ok();
        error.message.push_str(if restored {
            "; previous TUN settings and runtime restored"
        } else {
            "; TUN recovery was not verified; inspect its status before retrying"
        });
        let mut details = match error.details.take() {
            Some(serde_json::Value::Object(details)) => serde_json::Value::Object(details),
            cause => serde_json::json!({"cause": cause}),
        };
        details["tunRollback"] = serde_json::json!({
            "succeeded": restored,
            "error": recovery.err().map(|error| error.message),
        });
        error.details = Some(details);
        return Err(error);
    }
    Ok(())
}

async fn rollback(
    backend: &impl Backend,
    identity: &KernelRuntimeIdentity,
    previous: &AppConfig,
    candidate: &AppConfig,
) -> AppResult<()> {
    let current = owned(backend, identity).await?;
    if !matches(&current.status, &previous.tun) {
        if current.status.enabled {
            if !matches(&current.status, &candidate.tun) {
                return Err(conflict(
                    "TUN changed externally; refusing to replace another runtime",
                ));
            }
            transition(backend, identity, Some(&candidate.tun), None).await?;
        }
        transition(backend, identity, None, Some(&previous.tun)).await?;
    }
    backend.persist(previous)
}
