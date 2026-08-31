use crate::errors::{AppError, AppResult};
use crate::kernel::adapter::KernelAdapter;
use crate::kernel::zero::{self, ZeroAdapter};
use crate::state::app_state::AppState;

use super::default_opts;

pub(super) fn validate_apply_scope(
    before: &zero::queries::KernelRuntimeIdentity,
    applied: &zero::queries::KernelRuntimeIdentity,
    current: &zero::queries::KernelRuntimeIdentity,
) -> AppResult<()> {
    if before.core_instance_id == applied.core_instance_id && applied == current {
        return Ok(());
    }
    Err(AppError {
        code: "conflict",
        message: "DNS configuration result belongs to a stale core instance or config revision"
            .to_owned(),
        details: Some(serde_json::json!({
            "resource": "dns_config",
            "before": {
                "coreInstanceId": before.core_instance_id,
                "configRevision": before.config_revision,
            },
            "applied": {
                "coreInstanceId": applied.core_instance_id,
                "configRevision": applied.config_revision,
            },
            "current": {
                "coreInstanceId": current.core_instance_id,
                "configRevision": current.config_revision,
            },
        })),
    })
}

pub(super) fn rollback_scope_owned(
    expected: &zero::queries::KernelRuntimeIdentity,
    current: &zero::queries::KernelRuntimeIdentity,
) -> bool {
    expected == current
}

pub(super) async fn rollback_runtime_if_owned(
    state: &AppState,
    previous_effective: &serde_json::Value,
    expected: &zero::queries::KernelRuntimeIdentity,
    error: AppError,
) -> AppError {
    let current = match zero::queries::runtime_identity(Some(default_opts(state))).await {
        Ok(current) => current,
        Err(scope_error) => {
            return with_rollback_status(
                error,
                false,
                false,
                "current_scope_unavailable",
                Some(scope_error),
            );
        }
    };
    if !rollback_scope_owned(expected, &current) {
        return with_rollback_status(
            error,
            false,
            false,
            "current_scope_changed",
            None,
        );
    }

    let response = match ZeroAdapter::new()
        .apply_config(previous_effective.clone(), default_opts(state))
        .await
    {
        Ok(response) => response,
        Err(rollback_error) => {
            return with_rollback_status(
                error,
                true,
                false,
                "apply_failed",
                Some(rollback_error),
            );
        }
    };
    let rollback_applied = match zero::queries::config_apply_identity(&response) {
        Ok(identity) => identity,
        Err(rollback_error) => {
            return with_rollback_status(
                error,
                true,
                false,
                "rollback_identity_unavailable",
                Some(rollback_error),
            );
        }
    };
    let rollback_current = match zero::queries::runtime_identity(Some(default_opts(state))).await {
        Ok(identity) => identity,
        Err(rollback_error) => {
            return with_rollback_status(
                error,
                true,
                false,
                "rollback_scope_unavailable",
                Some(rollback_error),
            );
        }
    };
    if let Err(rollback_error) = validate_apply_scope(expected, &rollback_applied, &rollback_current)
    {
        return with_rollback_status(
            error,
            true,
            false,
            "rollback_scope_changed",
            Some(rollback_error),
        );
    }

    with_rollback_status(
        error,
        true,
        true,
        "restored_last_known_good",
        None,
    )
}

pub(super) fn with_rollback_status(
    mut error: AppError,
    attempted: bool,
    succeeded: bool,
    reason: &'static str,
    rollback_error: Option<AppError>,
) -> AppError {
    let rollback_error_value = rollback_error.map(|rollback_error| {
        serde_json::json!({
            "code": rollback_error.code,
            "message": rollback_error.message,
            "details": rollback_error.details,
        })
    });
    let mut details = match error.details.take() {
        Some(serde_json::Value::Object(details)) => details,
        Some(details) => {
            let mut object = serde_json::Map::new();
            object.insert("cause".to_owned(), details);
            object
        }
        None => serde_json::Map::new(),
    };
    details.insert(
        "dnsRollback".to_owned(),
        serde_json::json!({
            "attempted": attempted,
            "succeeded": succeeded,
            "reason": reason,
            "error": rollback_error_value,
        }),
    );
    error.details = Some(serde_json::Value::Object(details));
    if attempted && succeeded {
        error
            .message
            .push_str("; runtime restored to the last-known-good DNS configuration");
    } else if attempted {
        error.message.push_str(
            "; runtime rollback could not be verified; restart or reapply the current configuration",
        );
    } else {
        error.message.push_str(
            "; runtime rollback was skipped because ownership of the current core revision could not be proven",
        );
    }
    error
}

pub(super) fn with_storage_rollback_failure(
    mut error: AppError,
    stage: &'static str,
    rollback_error: AppError,
) -> AppError {
    let mut details = match error.details.take() {
        Some(serde_json::Value::Object(details)) => details,
        Some(details) => {
            let mut object = serde_json::Map::new();
            object.insert("cause".to_owned(), details);
            object
        }
        None => serde_json::Map::new(),
    };
    details.insert(
        "dnsStorageRollback".to_owned(),
        serde_json::json!({
            "succeeded": false,
            "stage": stage,
            "code": rollback_error.code,
            "message": rollback_error.message,
            "details": rollback_error.details,
        }),
    );
    error
        .message
        .push_str("; persisted DNS rollback failed and requires manual recovery");
    error.details = Some(serde_json::Value::Object(details));
    error
}

#[cfg(test)]
mod tests {
    use super::{rollback_scope_owned, validate_apply_scope, with_rollback_status};
    use crate::errors::AppError;
    use crate::kernel::zero::queries::KernelRuntimeIdentity;

    fn identity(core: &str, revision: u64) -> KernelRuntimeIdentity {
        KernelRuntimeIdentity {
            core_instance_id: core.to_owned(),
            config_revision: revision,
        }
    }

    #[test]
    fn apply_result_must_match_the_same_live_core_scope() {
        let before = identity("core-a", 6);
        let applied = identity("core-a", 7);
        assert!(validate_apply_scope(&before, &applied, &applied).is_ok());

        let restarted = identity("core-b", 1);
        let error = validate_apply_scope(&before, &restarted, &restarted)
            .expect_err("a restarted core invalidates the pending result");
        assert_eq!(error.code, "conflict");

        let newer = identity("core-a", 8);
        assert!(validate_apply_scope(&before, &applied, &newer).is_err());
        assert!(rollback_scope_owned(&applied, &applied));
        assert!(!rollback_scope_owned(&applied, &newer));
    }

    #[test]
    fn rollback_status_preserves_cause_and_reports_recovery() {
        let error = AppError {
            code: "conflict",
            message: "stale DNS apply".to_owned(),
            details: Some(serde_json::json!({ "resource": "dns_config" })),
        };
        let restored = with_rollback_status(
            error.clone(),
            true,
            true,
            "restored_last_known_good",
            None,
        );
        assert_eq!(restored.details.as_ref().unwrap()["resource"], "dns_config");
        assert_eq!(
            restored.details.as_ref().unwrap()["dnsRollback"]["succeeded"],
            true
        );
        assert!(restored.message.contains("last-known-good"));

        let skipped = with_rollback_status(
            error,
            false,
            false,
            "current_scope_changed",
            None,
        );
        assert_eq!(
            skipped.details.as_ref().unwrap()["dnsRollback"]["reason"],
            "current_scope_changed"
        );
        assert!(skipped.message.contains("ownership"));
    }
}
