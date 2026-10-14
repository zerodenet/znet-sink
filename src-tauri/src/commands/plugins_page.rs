// Included in the desktop-only plugin command module; mobile stays unavailable.
fn page_host(window: &tauri::Window) -> AppResult<()> {
    crate::services::native_documents::require_host(window.label())
}
#[tauri::command]
pub async fn plugins_page_document(
    app: AppHandle,
    window: tauri::Window,
    plugin_id: String,
    page_id: String,
) -> AppResult<crate::services::plugins::page_channel::PageDocument> {
    page_host(&window)?;
    blocking(app, move |state| {
        state
            .plugins()
            .page_document(state.capabilities(), &plugin_id, &page_id)
    })
    .await
}
#[tauri::command]
pub fn plugins_page_close(
    state: State<'_, AppState>,
    window: tauri::Window,
    document_review: String,
) -> AppResult<()> {
    page_host(&window)?;
    state.plugins().page_close(&document_review);
    Ok(())
}
#[tauri::command]
pub async fn plugins_page_check(
    app: AppHandle,
    window: tauri::Window,
    document_review: String,
) -> AppResult<()> {
    page_host(&window)?;
    blocking(app, move |state| {
        state
            .plugins()
            .page_check(state.capabilities(), &document_review)
    })
    .await
}
#[tauri::command]
pub async fn plugins_page_configure(
    app: AppHandle,
    window: tauri::Window,
    document_review: String,
    review: Review,
    values: BTreeMap<String, String>,
) -> AppResult<Snapshot> {
    page_host(&window)?;
    blocking(app, move |state| {
        let binding =
            state
                .plugins()
                .page_binding(state.capabilities(), &document_review, &review)?;
        let snapshot = state.plugins().configure_reviewed(
            state.capabilities(),
            review,
            values,
            binding.cancelled.clone(),
        )?;
        // Own configuration changes revoke the original epoch. Return the real
        // new snapshot, then retire this document rather than mint authority.
        binding.check()?;
        state.plugins().page_close(&document_review);
        Ok(snapshot)
    })
    .await
}
#[tauri::command]
pub async fn plugins_page_invoke(
    app: AppHandle,
    window: tauri::Window,
    document_review: String,
    review: Review,
    action: String,
    payload: serde_json::Value,
) -> AppResult<serde_json::Value> {
    page_host(&window)?;
    blocking(app.clone(), move |state| {
        let binding =
            state
                .plugins()
                .page_binding(state.capabilities(), &document_review, &review)?;
        let value = state.plugins().invoke_reviewed(
            state.capabilities(),
            Some(app),
            review,
            action,
            payload,
            binding.cancelled.clone(),
        )?;
        binding.check()?;
        state
            .plugins()
            .page_check(state.capabilities(), &document_review)?;
        Ok(value)
    })
    .await
}
#[tauri::command]
pub async fn plugins_page_file_preflight(
    app: AppHandle,
    window: tauri::Window,
    document_review: String,
    review: Review,
    call: SdkCall,
) -> Reply {
    let result = async {
        page_host(&window)?;
        if !matches!(call.method, Method::FileRead | Method::FileWrite) {
            return Err(crate::errors::AppError::invalid_argument(
                "此请求不是系统文件选择操作",
            ));
        }
        blocking(app, move |state| {
            let (binding, lease) = state.plugins().page_lease(
                state.capabilities(),
                &document_review,
                &review,
                &call,
            )?;
            binding.check_lease(&lease, Some(&call.request))?;
            Ok(serde_json::json!({"state":"permission-checked"}))
        })
        .await
    }
    .await;
    sdk_reply(result)
}
#[tauri::command]
pub async fn plugins_page_sdk_call(
    app: AppHandle,
    window: tauri::Window,
    document_review: String,
    review: Review,
    call: SdkCall,
) -> Reply {
    if let Err(error) = page_host(&window) {
        return sdk_reply(Err(error));
    }
    if call.method == Method::ProtectedLoad {
        return sdk_reply(Err(crate::errors::AppError::invalid_argument(
            "受保护加载须使用专属运行通道",
        )));
    }
    let method = call.method;
    let key = review.key.clone();
    let result = page_sdk_execute(app.clone(), document_review, review, call).await;
    let (plugin_id, component_id) = key.split_once('/').unwrap_or(("unprovided", "unprovided"));
    sdk_reply_with_log(&app, plugin_id, component_id, method, result)
}
async fn page_sdk_execute(
    app: AppHandle,
    document_review: String,
    review: Review,
    call: SdkCall,
) -> AppResult<serde_json::Value> {
    let method = call.method;
    let request = call.request.clone();
    let prepare_call = call.clone();
    let prepare_review = review.clone();
    let token = document_review.clone();
    let (binding, lease) = blocking(app.clone(), move |state| {
        let (binding, lease) = state.plugins().page_lease(
            state.capabilities(),
            &token,
            &prepare_review,
            &prepare_call,
        )?;
        Ok((binding, std::sync::Arc::new(lease)))
    })
    .await?;
    let plugin_id = binding.plugin_id.clone();
    let component_id = review
        .key
        .strip_prefix(&format!("{plugin_id}/"))
        .ok_or_else(|| crate::errors::AppError::invalid_argument("插件组件来源已变化"))?
        .to_owned();
    let result = match method {
        Method::SubscriptionApply => {
            let state = app.state::<AppState>();
            let (input, permission) = state
                .plugins()
                .managed_subscription_input_with_lease(&plugin_id, &lease, call)?;
            let auth_binding = binding.clone();
            let auth_lease = lease.clone();
            crate::services::subscription::apply_managed_authorized(app.clone(), input, move || {
                auth_binding.check_lease(&auth_lease, Some(&permission))
            })
            .await
            .and_then(|value| {
                serde_json::to_value(value)
                    .map_err(|_| crate::errors::AppError::internal("插件操作结果无法序列化"))
            })
        }
        Method::SubscriptionMetadataUpdate => {
            let state = app.state::<AppState>();
            let (input, permission) = state
                .plugins()
                .managed_subscription_metadata_with_lease(&plugin_id, &lease, call)?;
            let auth_binding = binding.clone();
            let auth_lease = lease.clone();
            let work_app = app.clone();
            blocking(app.clone(), move |_| {
                crate::services::subscription::update_managed_metadata_authorized(
                    work_app,
                    input,
                    move || auth_binding.check_lease(&auth_lease, Some(&permission)),
                )
                .and_then(|value| {
                    serde_json::to_value(value)
                        .map_err(|_| crate::errors::AppError::internal("插件操作结果无法序列化"))
                })
            })
            .await
        }
        Method::SubscriptionSyncComplete => {
            let state = app.state::<AppState>();
            let (input, permission) = state
                .plugins()
                .managed_subscription_sync_complete_with_lease(&plugin_id, &lease, call)?;
            let auth_binding = binding.clone();
            let auth_lease = lease.clone();
            let work_app = app.clone();
            blocking(app.clone(), move |_| {
                crate::services::subscription::complete_managed_sync_authorized(
                    work_app,
                    input,
                    move || auth_binding.check_lease(&auth_lease, Some(&permission)),
                )
                .and_then(|value| {
                    serde_json::to_value(value)
                        .map_err(|_| crate::errors::AppError::internal("插件操作结果无法序列化"))
                })
            })
            .await
        }
        Method::SubscriptionRemove => {
            let state = app.state::<AppState>();
            let (id, remove_config, permission) = state
                .plugins()
                .managed_subscription_removal_with_lease(&lease, call)?;
            let auth_binding = binding.clone();
            let auth_lease = lease.clone();
            crate::services::subscription::remove_managed_authorized(
                app.clone(),
                id,
                remove_config,
                plugin_id.clone(),
                move || auth_binding.check_lease(&auth_lease, Some(&permission)),
            )
            .await
            .and_then(|value| {
                serde_json::to_value(value)
                    .map_err(|_| crate::errors::AppError::internal("插件操作结果无法序列化"))
            })
        }
        _ => {
            let use_binding = binding.clone();
            let use_lease = lease.clone();
            let use_app = app.clone();
            let use_request = request.clone();
            let use_plugin = plugin_id.clone();
            blocking(app.clone(), move |state| {
                use_binding.check_lease(&use_lease, Some(&use_request))?;
                let network_proxy = crate::configuration::preferences::endpoint(state)
                    .ok()
                    .map(|(host, port)| format!("http://{host}:{port}"));
                let value = state.plugins().sdk_call_with_lease(
                    state.capabilities(),
                    &use_plugin,
                    &component_id,
                    &use_lease,
                    call,
                    network_proxy,
                    Some(state),
                )?;
                use_binding.check_lease(&use_lease, Some(&use_request))?;
                let value = if method == Method::BrowserOpen {
                    crate::services::plugins::sdk::open_browser_with_lease(
                        &use_app,
                        &use_lease,
                        &use_request,
                        value,
                    )?
                } else if method == Method::NotificationPost {
                    crate::services::plugins::notification::deliver_if_post(
                        method,
                        value,
                        |value| {
                            use_binding.check_lease(&use_lease, Some(&use_request))?;
                            crate::services::plugins::notification::publish(&use_app, value)
                        },
                    )?
                } else {
                    value
                };
                use_binding.check_lease(&use_lease, Some(&use_request))?;
                Ok(value)
            })
            .await
        }
    }?;
    binding.check_lease(&lease, Some(&request))?;
    let token = document_review.clone();
    blocking(app, move |state| {
        state.plugins().page_check(state.capabilities(), &token)
    })
    .await?;
    Ok(result)
}
#[tauri::command]
pub async fn plugins_page_protected_load(
    app: AppHandle,
    window: tauri::Window,
    document_review: String,
    review: Review,
    call: SdkCall,
) -> Reply {
    let result = async {
        use crate::kernel::adapter::KernelAdapter;
        page_host(&window)?;
        if call.method != Method::ProtectedLoad {
            return Err(crate::errors::AppError::invalid_argument(
                "受保护加载请求无效",
            ));
        }
        let request = call.request.clone();
        let prepare_call = call.clone();
        let token = document_review.clone();
        let (binding, lease, bytes) = blocking(app.clone(), move |state| {
            let (binding, lease) =
                state
                    .plugins()
                    .page_lease(state.capabilities(), &token, &review, &prepare_call)?;
            let component_id = review
                .key
                .strip_prefix(&format!("{}/", binding.plugin_id))
                .ok_or_else(|| crate::errors::AppError::invalid_argument("插件组件来源已变化"))?;
            binding.check_lease(&lease, Some(&prepare_call.request))?;
            let bytes = state.plugins().take_protected_material_with_lease(
                &binding.plugin_id,
                component_id,
                &lease,
                prepare_call,
            )?;
            Ok((binding, lease, bytes))
        })
        .await?;
        let content: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| {
            crate::errors::AppError::invalid_argument("受保护材料不是有效 JSON 配置")
        })?;
        if !content.is_object() {
            return Err(crate::errors::AppError::invalid_argument(
                "受保护配置必须是 JSON 对象",
            ));
        }
        let state = app.state::<AppState>();
        binding.check_lease(&lease, Some(&request))?;
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
        binding.check_lease(&lease, Some(&request))?;
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
        binding.check_lease(&lease, Some(&request))?;
        state
            .plugins()
            .page_check(state.capabilities(), &document_review)?;
        Ok(serde_json::json!({
            "state": "applied", "coreInstanceId": identity.core_instance_id,
            "configRevision": identity.config_revision,
        }))
    }
    .await;
    sdk_reply(result)
}
