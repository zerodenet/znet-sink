use super::super::dns_transaction::{validate_apply_scope, with_rollback_status};
use crate::commands::app_config::tun_settings::transaction::{self as tun, Snapshot};
use crate::errors::{AppError, AppResult};
use crate::kernel::zero::queries::KernelRuntimeIdentity;
use crate::models::app_config::AppConfig;
use serde_json::Value;
use std::future::Future;

pub(super) trait Backend: tun::Backend {
    fn apply_dns(
        &self,
        config: &Value,
    ) -> impl Future<Output = AppResult<(Value, KernelRuntimeIdentity)>> + Send;
}

fn conflict() -> AppError {
    AppError::conflict(
        "dns",
        "runtime",
        "DNS/TUN state changed; refresh before retrying",
    )
}

async fn observe(backend: &impl Backend, expected: &KernelRuntimeIdentity) -> AppResult<Snapshot> {
    let snapshot = backend.snapshot().await?;
    if snapshot.identity != *expected {
        return Err(conflict());
    }
    Ok(snapshot)
}

fn uncertain(error: &AppError) -> bool {
    matches!(
        error.code,
        "timeout" | "conflict" | "core_unavailable" | "connection_closed" | "dns_apply_uncertain"
    ) || error.message.contains("timed out")
        || error
            .details
            .as_ref()
            .is_some_and(|details| details["tunTransitionUncertain"] == true)
}

pub(super) async fn apply(
    backend: &impl Backend,
    previous: &AppConfig,
    candidate: &AppConfig,
    old_config: &Value,
    next_config: &Value,
) -> AppResult<Value> {
    let before = backend.snapshot().await?;
    let manage_tun = before.status.enabled && !before.status.managed_by_config;
    if manage_tun && !tun::matches(&before.status, &previous.tun) {
        return Err(conflict());
    }
    let mut expected = before.identity;
    let mut config_applied = false;
    let mut storage_attempted = false;
    let result = async {
        // Stop the old interception loop before removing/replacing its DNS backend.
        // Reuse the same verified, single-submission TUN transition as TUN settings.
        if manage_tun {
            tun::transition(backend, &expected, Some(&previous.tun), None).await?;
        }
        observe(backend, &expected).await?;
        let (value, applied) = backend.apply_dns(next_config).await?;
        validate_apply_scope(&expected, &applied, &backend.snapshot().await?.identity)?;
        expected = applied;
        config_applied = true;
        if manage_tun {
            tun::transition(backend, &expected, None, Some(&candidate.tun)).await?;
        }
        observe(backend, &expected).await?;
        storage_attempted = true;
        backend.persist(candidate)?;
        Ok(value)
    }
    .await;
    let Err(error) = result else {
        return result;
    };
    if uncertain(&error) {
        return Err(with_rollback_status(
            error,
            false,
            false,
            "completion_or_ownership_uncertain",
            None,
        ));
    }
    let recovery = async {
        let current = observe(backend, &expected).await?;
        // A rejected stop may leave the original healthy TUN untouched.
        if !config_applied
            && !storage_attempted
            && manage_tun
            && tun::matches(&current.status, &previous.tun)
        {
            return Ok(());
        }
        if manage_tun {
            if current.status.managed_by_config {
                return Err(conflict());
            }
            if current.status.enabled {
                let running = if config_applied {
                    &candidate.tun
                } else {
                    &previous.tun
                };
                tun::transition(backend, &expected, Some(running), None).await?;
            }
        }
        if config_applied {
            let (_, applied) = backend.apply_dns(old_config).await?;
            validate_apply_scope(&expected, &applied, &backend.snapshot().await?.identity)?;
            expected = applied;
        }
        if manage_tun {
            tun::transition(backend, &expected, None, Some(&previous.tun)).await?;
        }
        if storage_attempted {
            backend.persist(previous)?;
        }
        Ok::<_, AppError>(())
    }
    .await;
    Err(with_rollback_status(
        error,
        true,
        recovery.is_ok(),
        "dns_tun_storage_restore",
        recovery.err(),
    ))
}

#[cfg(test)]
mod tests;
