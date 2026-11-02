//! Publish endpoint preferences only after the real capture-aware apply receipt.
use super::*;
use crate::models::{app_config::AppConfig, core_process::CoreProcessState};
use crate::services::{app_config, core_config, core_process, rule_overlay};

pub(crate) async fn apply_endpoint_preferences(
    state: &AppState,
    previous: &AppConfig,
    next: AppConfig,
) -> AppResult<AppConfig> {
    let receipt = if core_process::refresh_status(state)?.state == CoreProcessState::Running {
        let (id, source) = crate::configuration::local_edits::context(state)?;
        if id.is_none() {
            return Err(AppError::invalid_argument("运行中的内核需要先选择配置"));
        }
        let plan = |app: &AppConfig| -> AppResult<(Value, Value)> {
            let config =
                rule_overlay::compose_candidate_with_app(state, &source, id.as_deref(), app)?
                    .config;
            let (resolved, source) =
                crate::configuration::local_edits::resolve(app, id.as_deref(), &source)?;
            let params = crate::configuration::preferences::tun_params_for(
                &resolved,
                &source,
                resolved.tun.clone(),
            )?;
            Ok((config, params))
        };
        let (previous_config, previous_params) = plan(previous)?;
        let (applied_config, applied_params) = plan(&next)?;
        let options = core_config::ipc_options_from_app_config(&previous.core);
        let identity = Box::pin(apply(
            &Live {
                state,
                options: options.clone(),
            },
            &previous_config,
            &applied_config,
            &previous_params,
            &applied_params,
        ))
        .await?;
        Some(Receipt {
            identity,
            previous_config,
            applied_config,
            previous_params,
            applied_params,
            options,
        })
    } else {
        None
    };
    if let Err(mut error) = app_config::replace(state, next.clone()) {
        if let Err(recovery) = app_config::replace(state, previous.clone()) {
            error
                .message
                .push_str(&format!("; settings rollback failed: {}", recovery.message));
        }
        if let Some(receipt) = receipt {
            if let Err(recovery) = receipt.restore(state).await {
                error
                    .message
                    .push_str(&format!("; runtime rollback failed: {}", recovery.message));
            }
        }
        return Err(error);
    }
    Ok(next)
}
