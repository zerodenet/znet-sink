#[tauri::command]
pub fn plugins_supported() -> bool {
    cfg!(not(any(target_os = "android", target_os = "ios")))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod desktop {
    use crate::{
        errors::AppResult,
        services::plugins::{InstallReview, Review, Snapshot},
        state::app_state::AppState,
    };
    use std::collections::BTreeMap;
    use tauri::{AppHandle, Emitter, Manager, State};
    use znet_plugin_sandbox::contract::Request;
    use znet_plugin_sandbox::sdk::{Call as SdkCall, Method, Reply, SDK_VERSION};

    fn sdk_reply(result: AppResult<serde_json::Value>) -> Reply {
        match result {
            Ok(value) => Reply::success(value),
            Err(error) => Reply {
                version: SDK_VERSION,
                ok: false,
                value: None,
                error: Some(crate::services::plugins::sdk::public_failure(error)),
            },
        }
    }

    fn sdk_reply_with_log(
        app: &AppHandle,
        plugin_id: &str,
        component_id: &str,
        method: Method,
        result: AppResult<serde_json::Value>,
    ) -> Reply {
        if result.is_err()
            || matches!(
                method,
                Method::SubscriptionApply
                    | Method::SubscriptionMetadataUpdate
                    | Method::SubscriptionRemove
                    | Method::ProtectedLoad
            )
        {
            let state = app.state::<AppState>();
            crate::services::logs::plugin_host_event(
                state.inner(),
                plugin_id,
                component_id,
                &format!("sdk.{method:?}"),
                result.as_ref().map(|_| ()),
            );
        }
        sdk_reply(result)
    }
    async fn blocking<T: Send + 'static>(
        app: AppHandle,
        work: impl FnOnce(&AppState) -> AppResult<T> + Send + 'static,
    ) -> AppResult<T> {
        tauri::async_runtime::spawn_blocking(move || work(app.state::<AppState>().inner()))
            .await
            .map_err(|_| crate::errors::AppError::internal("插件操作未能完成"))?
    }
    #[tauri::command]
    pub fn plugins_snapshot(state: State<'_, AppState>) -> Snapshot {
        state.plugins().snapshot(state.capabilities())
    }
    #[tauri::command]
    pub fn plugins_page(
        state: State<'_, AppState>,
        plugin_id: String,
        page_id: String,
    ) -> AppResult<String> {
        state.plugins().page(&plugin_id, &page_id)
    }
    #[tauri::command]
    pub async fn plugins_refresh(app: AppHandle) -> AppResult<Snapshot> {
        blocking(app, |state| state.plugins().refresh(state.capabilities())).await
    }
    #[tauri::command]
    pub async fn plugins_authorize(
        app: AppHandle,
        review: Review,
        grants: Vec<Request>,
    ) -> AppResult<Snapshot> {
        blocking(app, move |state| {
            state
                .plugins()
                .authorize(state.capabilities(), review, grants)
        })
        .await
    }
    #[tauri::command]
    pub async fn plugins_configure(
        app: AppHandle,
        key: String,
        values: BTreeMap<String, String>,
    ) -> AppResult<Snapshot> {
        blocking(app, move |state| {
            state
                .plugins()
                .configure(state.capabilities(), &key, values)
        })
        .await
    }
    #[tauri::command]
    pub fn plugins_stop(state: State<'_, AppState>, key: String) -> AppResult<Snapshot> {
        state.plugins().stop(state.capabilities(), &key)
    }
    #[tauri::command]
    pub fn plugins_revoke_permissions(
        state: State<'_, AppState>,
        key: String,
    ) -> AppResult<Snapshot> {
        state
            .plugins()
            .revoke_permissions(state.capabilities(), &key)
    }
    #[tauri::command]
    pub async fn plugins_storage_get(
        app: AppHandle,
        plugin_id: String,
        area: crate::services::plugins::namespace::Area,
        key: String,
    ) -> AppResult<Option<String>> {
        blocking(app, move |state| {
            state.plugins().storage_get(&plugin_id, area, &key)
        })
        .await
    }
    #[tauri::command]
    pub async fn plugins_storage_put(
        app: AppHandle,
        plugin_id: String,
        area: crate::services::plugins::namespace::Area,
        key: String,
        value: String,
    ) -> AppResult<()> {
        blocking(app, move |state| {
            state.plugins().storage_put(&plugin_id, area, key, value)
        })
        .await
    }
    #[tauri::command]
    pub async fn plugins_storage_delete(
        app: AppHandle,
        plugin_id: String,
        area: crate::services::plugins::namespace::Area,
        key: String,
    ) -> AppResult<()> {
        blocking(app, move |state| {
            state.plugins().storage_delete(&plugin_id, area, &key)
        })
        .await
    }
    #[tauri::command]
    pub async fn plugins_run(app: AppHandle, review: Review) -> AppResult<serde_json::Value> {
        blocking(app.clone(), move |state| {
            let owner = review.key.clone();
            let result = state
                .plugins()
                .run_with_host(state.capabilities(), review, Some(app));
            if let Some((plugin_id, component_id)) = owner.split_once('/') {
                crate::services::logs::plugin_host_event(
                    state,
                    plugin_id,
                    component_id,
                    "run",
                    result.as_ref().map(|_| ()),
                );
            }
            result
        })
        .await
    }
    #[tauri::command]
    pub async fn plugins_invoke(
        app: AppHandle,
        plugin_id: String,
        component_id: String,
        action: String,
        payload: serde_json::Value,
    ) -> AppResult<serde_json::Value> {
        blocking(app.clone(), move |state| {
            let result = state.plugins().invoke(
                state.capabilities(),
                Some(app.clone()),
                &plugin_id,
                &component_id,
                action.clone(),
                payload,
            );
            let log_action = if action.len() <= 128
                && action
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            {
                action.as_str()
            } else {
                "invalid-action"
            };
            crate::services::logs::plugin_host_event(
                state,
                &plugin_id,
                &component_id,
                log_action,
                result.as_ref().map(|_| ()),
            );
            result
        })
        .await
    }
    #[tauri::command]
    pub async fn plugins_sdk_call(
        app: AppHandle,
        plugin_id: String,
        component_id: String,
        call: SdkCall,
    ) -> Reply {
        if call.method == Method::SubscriptionApply {
            let timeout_ms = call.budget.timeout_ms;
            let managed_call = call.clone();
            let prepared = blocking(app.clone(), {
                let plugin_id = plugin_id.clone();
                let component_id = component_id.clone();
                move |state| {
                    state.plugins().prepare_managed_subscription(
                        state.capabilities(),
                        &plugin_id,
                        &component_id,
                        managed_call,
                    )
                }
            })
            .await;
            return match prepared {
                Ok(prepared) => sdk_reply_with_log(
                    &app,
                    &plugin_id,
                    &component_id,
                    call.method,
                    match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), {
                        let (input, authorization) = prepared.into_parts();
                        crate::services::subscription::apply_managed_authorized(
                            app.clone(),
                            input,
                            move || authorization.check(),
                        )
                    })
                    .await
                    {
                        Ok(result) => result.and_then(|profile| {
                            serde_json::to_value(profile).map_err(|error| {
                                crate::errors::AppError::internal(format!(
                                    "无法序列化托管订阅结果：{error}"
                                ))
                            })
                        }),
                        Err(_) => Err(crate::errors::AppError {
                            code: "plugin_sdk_deadline",
                            message: "插件托管订阅操作超时".into(),
                            details: None,
                        }),
                    },
                ),
                Err(error) => {
                    sdk_reply_with_log(&app, &plugin_id, &component_id, call.method, Err(error))
                }
            };
        }
        if call.method == Method::SubscriptionMetadataUpdate {
            let timeout_ms = call.budget.timeout_ms;
            let metadata_call = call.clone();
            let prepared = blocking(app.clone(), {
                let plugin_id = plugin_id.clone();
                let component_id = component_id.clone();
                move |state| {
                    state
                        .plugins()
                        .prepare_managed_subscription_metadata_update(
                            state.capabilities(),
                            &plugin_id,
                            &component_id,
                            metadata_call,
                        )
                }
            })
            .await;
            return match prepared {
                Ok(prepared) => sdk_reply_with_log(
                    &app,
                    &plugin_id,
                    &component_id,
                    call.method,
                    match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), {
                        let app = app.clone();
                        blocking(app.clone(), move |_| {
                            crate::services::subscription::update_managed_metadata_authorized(
                                app,
                                prepared.input,
                                move || prepared.authorization.check(),
                            )
                        })
                    })
                    .await
                    {
                        Ok(result) => result.and_then(|profile| {
                            serde_json::to_value(profile).map_err(|error| {
                                crate::errors::AppError::internal(format!(
                                    "无法序列化托管订阅用量结果：{error}",
                                ))
                            })
                        }),
                        Err(_) => Err(crate::errors::AppError {
                            code: "plugin_sdk_deadline",
                            message: "插件托管订阅用量操作超时".into(),
                            details: None,
                        }),
                    },
                ),
                Err(error) => {
                    sdk_reply_with_log(&app, &plugin_id, &component_id, call.method, Err(error))
                }
            };
        }
        if call.method == Method::SubscriptionRemove {
            let timeout_ms = call.budget.timeout_ms;
            let removal_call = call.clone();
            let prepared = blocking(app.clone(), {
                let plugin_id = plugin_id.clone();
                let component_id = component_id.clone();
                move |state| {
                    state.plugins().prepare_managed_subscription_removal(
                        state.capabilities(),
                        &plugin_id,
                        &component_id,
                        removal_call,
                    )
                }
            })
            .await;
            return match prepared {
                Ok(prepared) => sdk_reply_with_log(
                    &app,
                    &plugin_id,
                    &component_id,
                    call.method,
                    match tokio::time::timeout(
                        std::time::Duration::from_millis(timeout_ms),
                        crate::services::subscription::remove_managed_authorized(
                            app.clone(),
                            prepared.subscription_id,
                            prepared.remove_associated_config,
                            plugin_id.clone(),
                            move || prepared.authorization.check(),
                        ),
                    )
                    .await
                    {
                        Ok(result) => result.and_then(|outcome| {
                            serde_json::to_value(outcome).map_err(|error| {
                                crate::errors::AppError::internal(format!(
                                    "无法序列化托管订阅移除结果：{error}"
                                ))
                            })
                        }),
                        Err(_) => Err(crate::errors::AppError {
                            code: "plugin_sdk_deadline",
                            message: "插件托管订阅移除操作超时".into(),
                            details: None,
                        }),
                    },
                ),
                Err(error) => {
                    sdk_reply_with_log(&app, &plugin_id, &component_id, call.method, Err(error))
                }
            };
        }
        let method = call.method;
        let log_plugin_id = plugin_id.clone();
        let log_component_id = component_id.clone();
        let result = blocking(app.clone(), move |state| {
            let network_proxy = crate::configuration::preferences::endpoint(state)
                .ok()
                .map(|(host, port)| format!("http://{host}:{port}"));
            state.plugins().sdk_call_with_network_proxy(
                state.capabilities(),
                &plugin_id,
                &component_id,
                call,
                network_proxy,
                Some(state),
            )
        })
        .await;
        sdk_reply_with_log(&app, &log_plugin_id, &log_component_id, method, result)
    }

    #[tauri::command]
    pub async fn plugins_protected_load(
        app: AppHandle,
        plugin_id: String,
        component_id: String,
        call: SdkCall,
    ) -> Reply {
        use crate::kernel::adapter::KernelAdapter;
        let budget = call.budget;
        let started = std::time::Instant::now();
        let extracted = blocking(app.clone(), {
            let plugin_id = plugin_id.clone();
            let component_id = component_id.clone();
            move |state| {
                state.plugins().take_protected_material(
                    state.capabilities(),
                    &plugin_id,
                    &component_id,
                    call,
                )
            }
        })
        .await;
        let (lease, bytes) = match extracted {
            Ok(value) => value,
            Err(error) => {
                return sdk_reply_with_log(
                    &app,
                    &plugin_id,
                    &component_id,
                    Method::ProtectedLoad,
                    Err(error),
                )
            }
        };
        let result = async {
            let content: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| {
                crate::errors::AppError::invalid_argument("受保护材料不是有效的 JSON 配置")
            })?;
            if !content.is_object() {
                return Err(crate::errors::AppError::invalid_argument(
                    "受保护配置必须是 JSON 对象",
                ));
            }
            let state = app.state::<AppState>();
            crate::configuration::preferences::require_capture_compatible(
                state.inner(),
                &content,
                None,
            )
            .await?;
            let content = crate::services::rule_overlay::compose_effective_config_for(
                state.inner(),
                &content,
                None,
            )?;
            let options = {
                let config = crate::services::common::lock(state.app_config(), "app_config")?;
                crate::services::core_config::ipc_options_from_app_config(&config.core)
            };
            crate::kernel::zero::ZeroAdapter::new()
                .validate_config(content.clone(), options.clone())
                .await?;
            let identity = crate::configuration::apply::apply_authorized_for(
                lease.host_lease(),
                znet_client_core::capability::Permission::new(
                    "runtime.protected.load",
                    "active-runtime",
                ),
                content,
                options,
            )
            .await?;
            lease.check(None).map_err(|error| {
                crate::errors::AppError::invalid_argument(format!("受保护配置授权已失效：{error}"))
            })?;
            Ok(serde_json::json!({
                "state":"applied",
                "coreInstanceId":identity.core_instance_id,
                "configRevision":identity.config_revision,
            }))
        }
        .await;
        if started.elapsed() > std::time::Duration::from_millis(budget.timeout_ms) {
            return sdk_reply_with_log(
                &app,
                &plugin_id,
                &component_id,
                Method::ProtectedLoad,
                Err(crate::errors::AppError {
                    code: "plugin_sdk_deadline",
                    message: "插件 SDK 操作超过声明的时间预算".into(),
                    details: None,
                }),
            );
        }
        sdk_reply_with_log(
            &app,
            &plugin_id,
            &component_id,
            Method::ProtectedLoad,
            result,
        )
    }
    #[tauri::command]
    pub async fn plugins_preview_install(app: AppHandle, path: String) -> AppResult<InstallReview> {
        blocking(app, move |state| {
            state.plugins().preview_install(state.capabilities(), path)
        })
        .await
    }
    #[tauri::command]
    pub async fn plugins_install(
        app: AppHandle,
        path: String,
        approval_digest: Option<String>,
    ) -> AppResult<Snapshot> {
        blocking(app, move |state| {
            state
                .plugins()
                .install(state.capabilities(), path, approval_digest)
        })
        .await
    }
    #[tauri::command]
    pub async fn plugins_catalog(
        app: AppHandle,
    ) -> AppResult<Vec<znet_plugin_sandbox::distribution::directory::Registration>> {
        blocking(app, |state| state.plugins().catalog(state.capabilities())).await
    }
    #[tauri::command]
    pub async fn plugins_releases(
        app: AppHandle,
        id: String,
    ) -> AppResult<Vec<znet_plugin_sandbox::distribution::remote::Release>> {
        blocking(app, move |state| {
            state.plugins().releases(state.capabilities(), &id)
        })
        .await
    }
    #[tauri::command]
    pub async fn plugins_install_release(
        app: AppHandle,
        id: String,
        tag: String,
        approval_digest: Option<String>,
    ) -> AppResult<Snapshot> {
        let progress_app = app.clone();
        let progress_id = id.clone();
        let progress_tag = tag.clone();
        blocking(app, move |state| {
            state.plugins().install_release(
                state.capabilities(),
                &id,
                &tag,
                approval_digest.as_deref(),
                move |progress| {
                    let percent = progress
                        .bytes_total
                        .filter(|total| *total > 0)
                        .map(|total| progress.bytes_downloaded as f64 / total as f64 * 100.0);
                    let _ = progress_app.emit(
                        "plugin:download-progress",
                        serde_json::json!({
                            "pluginId": progress_id.clone(),
                            "tag": progress_tag.clone(),
                            "bytesDownloaded": progress.bytes_downloaded,
                            "bytesTotal": progress.bytes_total,
                            "percent": percent,
                            "state": progress.state,
                            "attempt": progress.attempt,
                        }),
                    );
                },
            )
        })
        .await
    }
    #[tauri::command]
    pub async fn plugins_preview_release(
        app: AppHandle,
        id: String,
        tag: String,
    ) -> AppResult<InstallReview> {
        let progress_app = app.clone();
        let progress_id = id.clone();
        let progress_tag = tag.clone();
        blocking(app, move |state| {
            state
                .plugins()
                .preview_release(state.capabilities(), &id, &tag, move |progress| {
                    let percent = progress
                        .bytes_total
                        .filter(|total| *total > 0)
                        .map(|total| progress.bytes_downloaded as f64 / total as f64 * 100.0);
                    let _ = progress_app.emit(
                        "plugin:download-progress",
                        serde_json::json!({
                            "pluginId": progress_id.clone(),
                            "tag": progress_tag.clone(),
                            "bytesDownloaded": progress.bytes_downloaded,
                            "bytesTotal": progress.bytes_total,
                            "percent": percent,
                            "state": progress.state,
                            "attempt": progress.attempt,
                        }),
                    );
                })
        })
        .await
    }
    #[tauri::command]
    pub async fn plugins_uninstall(app: AppHandle, id: String) -> AppResult<Snapshot> {
        blocking(app, move |state| {
            state.plugins().uninstall(state.capabilities(), &id)
        })
        .await
    }
}
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use desktop::*;
