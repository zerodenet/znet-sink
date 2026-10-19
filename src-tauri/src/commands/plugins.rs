#[tauri::command]
pub fn plugins_supported() -> bool {
    cfg!(any(
        target_os = "android",
        target_os = "ios",
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    ))
}

mod host {
    use crate::models::logs::PluginAction;
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
                    | Method::SubscriptionSyncComplete
                    | Method::SubscriptionRemove
                    | Method::ProtectedLoad
            )
        {
            let state = app.state::<AppState>();
            crate::services::logs::plugin_host_event(
                state.inner(),
                plugin_id,
                component_id,
                crate::services::logs::plugin_sdk_action(method),
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
    include!("plugins_page.rs");

    #[tauri::command]
    pub fn plugins_snapshot(state: State<'_, AppState>) -> Snapshot {
        state.plugins().snapshot(state.capabilities())
    }
    #[tauri::command]
    pub async fn plugins_native_vm_smoke(
        app: AppHandle,
        window: tauri::Window,
    ) -> AppResult<serde_json::Value> {
        crate::services::native_documents::require_host(window.label())?;
        #[cfg(debug_assertions)]
        {
            let native_app = app.clone();
            blocking(app, move |state| {
                state.plugins().native_vm_smoke(Some(native_app))
            })
            .await
        }
        #[cfg(not(debug_assertions))]
        {
            let _ = app;
            Err(crate::errors::AppError::invalid_argument(
                "发行构建不提供固定 VM 验收",
            ))
        }
    }
    #[tauri::command]
    pub fn plugins_host_recheck(app: AppHandle, window: tauri::Window) -> AppResult<Snapshot> {
        crate::services::native_documents::require_host(window.label())?;
        #[cfg(any(target_os = "android", target_os = "ios"))]
        crate::services::plugins::recheck_native(&app)?;
        let state = app.state::<AppState>();
        Ok(state.plugins().snapshot(state.capabilities()))
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
        review: Option<Review>,
    ) -> AppResult<Snapshot> {
        blocking(app, move |state| {
            if let Some(review) = review {
                if review.key != key {
                    return Err(crate::errors::AppError::invalid_argument(
                        "插件确认目标已变化",
                    ));
                }
                state.plugins().configure_reviewed(
                    state.capabilities(),
                    review,
                    values,
                    std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                )
            } else if cfg!(any(target_os = "android", target_os = "ios")) {
                Err(crate::errors::AppError::invalid_argument(
                    "移动插件配置需要当前原生宿主的组件确认",
                ))
            } else {
                state
                    .plugins()
                    .configure(state.capabilities(), &key, values)
            }
        })
        .await
    }
    #[tauri::command]
    pub fn plugins_stop(
        state: State<'_, AppState>,
        key: String,
        review: Option<Review>,
    ) -> AppResult<Snapshot> {
        match review {
            Some(review) if review.key == key => {
                state.plugins().stop_reviewed(state.capabilities(), &review)
            }
            None if !cfg!(any(target_os = "android", target_os = "ios")) => {
                state.plugins().stop(state.capabilities(), &key)
            }
            _ => Err(crate::errors::AppError::invalid_argument(
                "插件确认目标或原生宿主已变化，请重新核对",
            )),
        }
    }
    #[tauri::command]
    pub fn plugins_revoke_permissions(
        state: State<'_, AppState>,
        key: String,
        review: Option<Review>,
    ) -> AppResult<Snapshot> {
        match review {
            Some(review) if review.key == key => state
                .plugins()
                .revoke_permissions_reviewed(state.capabilities(), &review),
            None if !cfg!(any(target_os = "android", target_os = "ios")) => state
                .plugins()
                .revoke_permissions(state.capabilities(), &key),
            _ => Err(crate::errors::AppError::invalid_argument(
                "插件确认目标或原生宿主已变化，请重新核对",
            )),
        }
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
                    PluginAction::Run,
                    result.as_ref().map(|_| ()),
                );
            }
            result
        })
        .await
    }
    /// All guest-selected VM action names share one component category. An
    /// action called "uninstall" or "sdk.SubscriptionApply" is still not the
    /// owning uninstall command or a typed SDK dispatch.
    fn invoke_component_with_log(
        state: &AppState,
        app: Option<AppHandle>,
        plugin_id: &str,
        component_id: &str,
        action: String,
        payload: serde_json::Value,
    ) -> AppResult<serde_json::Value> {
        let result = state.plugins().invoke(
            state.capabilities(),
            app,
            plugin_id,
            component_id,
            action,
            payload,
        );
        crate::services::logs::plugin_host_event(
            state,
            plugin_id,
            component_id,
            PluginAction::ComponentAction,
            result.as_ref().map(|_| ()),
        );
        result
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
            invoke_component_with_log(state, Some(app), &plugin_id, &component_id, action, payload)
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
        if call.method == Method::SubscriptionSyncComplete {
            let timeout_ms = call.budget.timeout_ms;
            let completion_call = call.clone();
            let prepared = blocking(app.clone(), {
                let plugin_id = plugin_id.clone();
                let component_id = component_id.clone();
                move |state| {
                    state.plugins().prepare_managed_subscription_sync_complete(
                        state.capabilities(),
                        &plugin_id,
                        &component_id,
                        completion_call,
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
                            crate::services::subscription::complete_managed_sync_authorized(
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
                                    "无法序列化托管订阅同步完成结果：{error}",
                                ))
                            })
                        }),
                        Err(_) => Err(crate::errors::AppError {
                            code: "plugin_sdk_deadline",
                            message: "插件托管订阅同步完成操作超时".into(),
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
        let result = if method == Method::NotificationPost {
            match result {
                Ok(value) => {
                    let delivery_app = app.clone();
                    blocking(app.clone(), move |_| {
                        crate::services::plugins::notification::deliver_if_post(
                            method,
                            value,
                            |value| {
                                crate::services::plugins::notification::publish(
                                    &delivery_app,
                                    value,
                                )
                            },
                        )
                    })
                    .await
                }
                Err(error) => Err(error),
            }
        } else {
            result
        };
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
                let activity = state.plugins().activity(&plugin_id)?;
                let (lease, bytes) = state.plugins().take_protected_material(
                    state.capabilities(),
                    &plugin_id,
                    &component_id,
                    call,
                )?;
                Ok((lease, bytes, activity))
            }
        })
        .await;
        let (lease, bytes, _activity) = match extracted {
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
    pub async fn plugins_native_package_pick(
        app: AppHandle,
        window: tauri::Window,
    ) -> AppResult<Option<crate::services::plugins::native_install::Preview>> {
        crate::services::native_documents::require_host(window.label())?;
        use tauri_plugin_znet_native_io::{ImportPurpose, NativeIoExt, ReceiptStatus};
        let native_app = app.clone();
        let receipt = tauri::async_runtime::spawn_blocking(move || {
            native_app
                .state::<AppState>()
                .plugins()
                .require_native_installation()?;
            let mut bytes = [0u8; 16];
            getrandom::fill(&mut bytes)
                .map_err(|_| crate::errors::AppError::internal("无法创建系统选择操作"))?;
            let id = bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            native_app
                .native_io()
                .pick_read(
                    &id,
                    ImportPurpose::PluginPackage,
                    znet_plugin_sandbox::distribution::MAX_PACKAGE_BYTES as u32,
                )
                .map_err(|_| {
                    crate::errors::AppError::internal(
                        "系统文件选择结果未确认，请先核对，不会自动重试",
                    )
                })
        })
        .await
        .map_err(|_| crate::errors::AppError::internal("系统文件选择未能完成"))??;
        if receipt.status == ReceiptStatus::Cancelled {
            return Ok(None);
        }
        let bytes = receipt
            .read_bytes()
            .map_err(|_| crate::errors::AppError::invalid_argument("插件包读取回执无效"))?
            .ok_or_else(|| {
                crate::errors::AppError::invalid_argument(
                    "系统未完成插件包读取；请检查文件权限或大小后重新选择",
                )
            })?;
        // Only a trusted native resume may reopen the host after the picker.
        for _ in 0..30 {
            if app
                .state::<AppState>()
                .plugins()
                .require_native_installation()
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        blocking(app, move |state| {
            state
                .plugins()
                .preview_native_package(state.capabilities(), bytes)
                .map(Some)
        })
        .await
    }
    #[tauri::command]
    pub async fn plugins_native_package_confirm(
        app: AppHandle,
        window: tauri::Window,
        plan_id: String,
        approval_digest: String,
    ) -> AppResult<Snapshot> {
        crate::services::native_documents::require_host(window.label())?;
        blocking(app, move |state| {
            state
                .plugins()
                .install_native_package(state.capabilities(), &plan_id, &approval_digest)
        })
        .await
    }
    #[tauri::command]
    pub fn plugins_native_package_cancel(
        state: State<'_, AppState>,
        window: tauri::Window,
        plan_id: String,
    ) -> AppResult<()> {
        crate::services::native_documents::require_host(window.label())?;
        state.plugins().cancel_native_package(&plan_id);
        Ok(())
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
        host_generation: Option<String>,
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
                host_generation.as_deref(),
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
        let _ = (app, id);
        Err(crate::errors::AppError {
            code: "plugin_uninstall_preview_required",
            message: "卸载必须先在主应用预览固定资源并明确确认；旧直接卸载入口已关闭".into(),
            details: None,
        })
    }
    fn uninstall_public_error(error: crate::errors::AppError) -> crate::errors::AppError {
        match error.code {
            "plugin_uninstall_preflight_rejected"
            | "plugin_uninstall_pending"
            | "plugin_uninstall_storage" => error,
            "not_found" => crate::errors::AppError {
                code: "not_found",
                message: "没有可供核对的原卸载记录；未重放任何删除".into(),
                details: None,
            },
            _ => crate::errors::AppError {
                code: "plugin_uninstall_storage",
                message: "卸载材料或执行结果无法安全核对；请保留原操作，仅查看记录，不会自动重放"
                    .into(),
                details: None,
            },
        }
    }
    #[tauri::command]
    pub async fn plugins_uninstall_preview(
        app: AppHandle,
        window: tauri::Window,
        plugin_id: String,
    ) -> AppResult<crate::models::plugin_uninstall::Preview> {
        page_host(&window)?;
        blocking(app, move |state| {
            state.plugins().uninstall_preview(state, &plugin_id)
        })
        .await
        .map_err(uninstall_public_error)
    }
    #[tauri::command]
    pub async fn plugins_uninstall_execute(
        app: AppHandle,
        window: tauri::Window,
        plan_id: String,
        operation_id: String,
        confirm_owned_resources: bool,
    ) -> AppResult<crate::models::plugin_uninstall::Status> {
        page_host(&window)?;
        let state = app.state::<AppState>();
        let _operation = state.proxy_config_operation().lock().await;
        blocking(app.clone(), move |state| {
            let result = state.plugins().uninstall_execute(
                state,
                &plan_id,
                &operation_id,
                confirm_owned_resources,
            );
            let failure = crate::errors::AppError {
                code: "plugin_uninstall_incomplete",
                message: "原卸载未取得全部固定项目的完成回执".into(),
                details: None,
            };
            if let Ok(status) = &result {
                crate::services::logs::plugin_host_event(
                    state,
                    &status.plugin_id,
                    "host",
                    PluginAction::Uninstall,
                    if status.state == "completed" {
                        Ok(())
                    } else {
                        Err(&failure)
                    },
                );
            }
            result
        })
        .await
        .map_err(uninstall_public_error)
    }
    #[tauri::command]
    pub async fn plugins_uninstall_status(
        app: AppHandle,
        window: tauri::Window,
        operation_id: String,
    ) -> AppResult<crate::models::plugin_uninstall::Status> {
        page_host(&window)?;
        blocking(app, move |state| {
            state.plugins().uninstall_status(&operation_id)
        })
        .await
        .map_err(uninstall_public_error)
    }
    #[tauri::command]
    pub async fn plugins_uninstall_recent(
        app: AppHandle,
        window: tauri::Window,
    ) -> AppResult<Vec<crate::models::plugin_uninstall::Status>> {
        page_host(&window)?;
        blocking(app, move |state| state.plugins().uninstall_recent())
            .await
            .map_err(uninstall_public_error)
    }
    #[tauri::command]
    pub async fn plugins_uninstall_discard(
        app: AppHandle,
        window: tauri::Window,
        plan_id: String,
    ) -> AppResult<()> {
        page_host(&window)?;
        blocking(app, move |state| {
            state.plugins().uninstall_discard(&plan_id)
        })
        .await
        .map_err(uninstall_public_error)
    }
    #[cfg(test)]
    mod diagnostic_tests {
        use super::*;
        #[test]
        fn guest_known_action_name_collisions_are_only_generic_component_attempts() {
            use crate::models::logs::{HostDiagnosticEvent, HostOutcome};
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("logs.jsonl");
            crate::services::log_store::with_test_path(&path, || {
                let state = AppState::default();
                for action in [
                    "run",
                    "uninstall",
                    "sdk.SubscriptionApply",
                    "sdk.SubscriptionMetadataUpdate",
                    "sdk.SubscriptionSyncComplete",
                    "sdk.SubscriptionRemove",
                    "sdk.ProtectedLoad",
                    "scheduled.secret-canary",
                ] {
                    let result = invoke_component_with_log(
                        &state,
                        None,
                        "missing.diagnostic.collision",
                        "worker",
                        action.into(),
                        serde_json::json!({"secret":"secret-canary"}),
                    );
                    assert!(
                        result.is_err(),
                        "No registered component may execute or gain SDK permission"
                    );
                    let entry = state.logs().lock().unwrap().last().unwrap().clone();
                    assert_eq!(
                        entry.diagnostic_event,
                        Some(HostDiagnosticEvent::Plugin {
                            action: PluginAction::ComponentAction,
                            outcome: HostOutcome::Failed
                        })
                    );
                }
                let reopened =
                    crate::services::log_store::load_recent_from_path(&path, 20).unwrap();
                assert_eq!(reopened.len(), 8);
                assert!(reopened
                    .iter()
                    .all(|entry| entry.diagnostic_event.is_some_and(|event| matches!(
                        event,
                        HostDiagnosticEvent::Plugin {
                            action: PluginAction::ComponentAction,
                            ..
                        }
                    ))));
            });
        }
    }
}
pub use host::*;
