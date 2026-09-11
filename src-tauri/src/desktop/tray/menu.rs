use super::environment::tray_copy_proxy_environment;
#[cfg(target_os = "windows")]
use super::environment::tray_open_proxy_terminal;
use super::{emit_tray_action, open_main_window_route, toggle_main_window, TrayMenuItems};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

pub(crate) fn install(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // System tray
    let show_item = tauri::menu::MenuItemBuilder::new("打开 ZNet Sink")
        .id("show")
        .build(app)?;
    let status_item = tauri::menu::MenuItemBuilder::new("状态：正在初始化")
        .id("status")
        .enabled(false)
        .build(app)?;
    let profile_item = tauri::menu::MenuItemBuilder::new("当前配置：读取中")
        .id("profile")
        .enabled(false)
        .build(app)?;
    let toggle_proxy_item = tauri::menu::MenuItemBuilder::new("开启代理")
        .id("toggle_proxy")
        .enabled(false)
        .build(app)?;
    let system_proxy_item = tauri::menu::CheckMenuItemBuilder::new("系统代理")
        .id("toggle_system_proxy")
        .enabled(false)
        .checked(false)
        .build(app)?;
    let tun_item = tauri::menu::CheckMenuItemBuilder::new("TUN 模式")
        .id("toggle_tun")
        .enabled(false)
        .checked(false)
        .build(app)?;
    let restart_core_item = tauri::menu::MenuItemBuilder::new("重启内核")
        .id("restart_core")
        .enabled(false)
        .build(app)?;
    let copy_proxy_env_item = tauri::menu::MenuItemBuilder::new("复制代理环境变量")
        .id("copy_proxy_env")
        .build(app)?;
    #[cfg(target_os = "windows")]
    let open_proxy_terminal_item = tauri::menu::MenuItemBuilder::new("打开终端")
        .id("open_proxy_terminal")
        .build(app)?;
    let settings_item = tauri::menu::MenuItemBuilder::new("设置")
        .id("settings")
        .build(app)?;
    let overview_item = tauri::menu::MenuItemBuilder::new("概览")
        .id("overview")
        .build(app)?;
    let nodes_item = tauri::menu::MenuItemBuilder::new("节点")
        .id("nodes")
        .build(app)?;
    let subscriptions_item = tauri::menu::MenuItemBuilder::new("订阅")
        .id("subscriptions")
        .build(app)?;
    let logs_item = tauri::menu::MenuItemBuilder::new("日志")
        .id("logs")
        .build(app)?;
    let quit_item = tauri::menu::MenuItemBuilder::new("退出")
        .id("quit")
        .build(app)?;

    let proxy_controls = tauri::menu::SubmenuBuilder::new(app, "代理控制")
        .items(&[&system_proxy_item, &tun_item])
        .build()?;
    let shortcuts = tauri::menu::SubmenuBuilder::new(app, "快捷入口")
        .items(&[&overview_item, &nodes_item, &subscriptions_item, &logs_item])
        .build()?;

    #[cfg(target_os = "windows")]
    let tools = tauri::menu::SubmenuBuilder::new(app, "诊断与工具")
        .items(&[
            &restart_core_item,
            &copy_proxy_env_item,
            &open_proxy_terminal_item,
        ])
        .build()?;

    #[cfg(not(target_os = "windows"))]
    let tools = tauri::menu::SubmenuBuilder::new(app, "诊断与工具")
        .items(&[&restart_core_item, &copy_proxy_env_item])
        .build()?;

    let tray_menu = tauri::menu::Menu::with_items(
        app,
        &[
            &show_item,
            &tauri::menu::PredefinedMenuItem::separator(app)?,
            &status_item,
            &profile_item,
            &tauri::menu::PredefinedMenuItem::separator(app)?,
            &toggle_proxy_item,
            &proxy_controls,
            &tauri::menu::PredefinedMenuItem::separator(app)?,
            &shortcuts,
            &settings_item,
            &tools,
            &tauri::menu::PredefinedMenuItem::separator(app)?,
            &quit_item,
        ],
    )?;

    // Hold references to the status-dependent items so
    // `tray_update_status` can toggle their enabled state.
    app.manage(TrayMenuItems {
        status: status_item,
        profile: profile_item,
        toggle_proxy: toggle_proxy_item,
        system_proxy: system_proxy_item,
        tun: tun_item,
        restart_core: restart_core_item,
    });

    let _tray_menu = TrayIconBuilder::with_id("main-tray")
        .tooltip("ZNet Sink · 已停止")
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&tray_menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" | "overview" => open_main_window_route(app, "overview", None),
            "toggle_proxy" => emit_tray_action(app, "toggle_proxy"),
            "toggle_system_proxy" => emit_tray_action(app, "toggle_system_proxy"),
            "toggle_tun" => emit_tray_action(app, "toggle_tun"),
            "restart_core" => emit_tray_action(app, "restart_core"),
            "nodes" => open_main_window_route(app, "nodes", None),
            "subscriptions" => open_main_window_route(app, "subscriptions", None),
            "logs" => open_main_window_route(app, "logs", None),
            "copy_proxy_env" => tray_copy_proxy_environment(app),
            #[cfg(target_os = "windows")]
            "open_proxy_terminal" => tray_open_proxy_terminal(app.clone()),
            "settings" => open_main_window_route(app, "settings", Some("general")),
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button,
                button_state,
                ..
            } = event
            {
                if button == MouseButton::Left && button_state == MouseButtonState::Up {
                    toggle_main_window(tray.app_handle());
                }
            }
        })
        .build(app)?;

    Ok(())
}
