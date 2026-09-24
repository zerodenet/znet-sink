//! Host delivery for a plugin notification after its SDK permission and payload checks.
use crate::errors::{AppError, AppResult};
use serde_json::Value;
use tauri::{AppHandle, Emitter as _};
use znet_plugin_sandbox::sdk::Method;

#[cfg(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd"
))]
mod desktop;

#[cfg(target_os = "macos")]
mod macos_foreground {
    use block2::DynBlock;
    use objc2::{define_class, msg_send, rc::Retained, runtime::ProtocolObject, MainThreadOnly};
    use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol};
    use objc2_user_notifications::{
        UNNotification, UNNotificationPresentationOptions, UNUserNotificationCenter,
        UNUserNotificationCenterDelegate,
    };

    define_class!(
        // SAFETY: NSObject has no subclassing requirements and this class has no Drop impl.
        #[unsafe(super = NSObject)]
        #[thread_kind = MainThreadOnly]
        #[ivars = ()]
        struct PluginNotificationDelegate;

        // SAFETY: These protocols have no additional implementation invariants.
        unsafe impl NSObjectProtocol for PluginNotificationDelegate {}
        unsafe impl UNUserNotificationCenterDelegate for PluginNotificationDelegate {
            #[allow(non_snake_case)]
            #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
            fn userNotificationCenter_willPresentNotification_withCompletionHandler(
                &self,
                _center: &UNUserNotificationCenter,
                _notification: &UNNotification,
                completion_handler: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
            ) {
                completion_handler.call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List,));
            }
        }
    );

    impl PluginNotificationDelegate {
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            // SAFETY: NSObject's init method has this signature.
            unsafe { msg_send![super(Self::alloc(mtm).set_ivars(())), init] }
        }
    }

    pub(super) fn install() {
        let Some(mtm) = MainThreadMarker::new() else {
            crate::services::file_logger::line(
                "plugin notifications: foreground delegate requires the main thread",
            );
            return;
        };
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let delegate = PluginNotificationDelegate::new(mtm);
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        // UNUserNotificationCenter holds a weak delegate reference for the app lifetime.
        std::mem::forget(delegate);
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn install_foreground_delegate() {
    macos_foreground::install();
}

pub(crate) fn deliver_if_post(
    method: Method,
    value: Value,
    deliver: impl FnOnce(&Value) -> AppResult<()>,
) -> AppResult<Value> {
    if method == Method::NotificationPost {
        deliver(&value)?;
    }
    Ok(value)
}

pub(crate) fn publish(app: &AppHandle, value: &Value) -> AppResult<()> {
    // Show the in-app feedback as soon as the SDK call is authorized. OS
    // permission prompts may take longer, and a hidden window must not block
    // the independent system delivery attempt.
    let _ = app.emit("plugin:notification", value.clone());

    let title = value
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("ZNet Sink");
    let body = value
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("插件有一条新通知");
    show_system(app, title, body)
}

#[cfg(target_os = "macos")]
fn show_system(_app: &AppHandle, title: &str, body: &str) -> AppResult<()> {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest,
        UNUserNotificationCenter,
    };
    use std::{
        sync::{atomic::AtomicU64, mpsc, Arc, Mutex},
        time::Duration,
    };

    const AUTH_WAIT: Duration = Duration::from_secs(30);
    const SUBMIT_WAIT: Duration = Duration::from_secs(10);
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let (sender, receiver) = mpsc::channel();
    let sender = Arc::new(Mutex::new(Some(sender)));
    let authorization = RcBlock::new(move |granted: Bool, error: *mut NSError| {
        if let Some(sender) = sender.lock().unwrap().take() {
            let _ = sender.send(if error.is_null() {
                Ok(granted.as_bool())
            } else {
                Err(())
            });
        }
    });
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
        &authorization,
    );
    let granted = receiver
        .recv_timeout(AUTH_WAIT)
        .map_err(|error| unavailable(format!("等待 macOS 通知授权超时：{error}")))?
        .map_err(|()| unavailable("macOS 通知授权请求失败".into()))?;
    if !granted {
        return Err(denied());
    }

    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setBody(&NSString::from_str(body));
    let identifier = NSString::from_str(&format!(
        "znet-plugin-{}-{}",
        crate::services::common::now_unix_ms(),
        NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let request =
        UNNotificationRequest::requestWithIdentifier_content_trigger(&identifier, &content, None);
    let (sender, receiver) = mpsc::channel();
    let sender = Arc::new(Mutex::new(Some(sender)));
    let completion = RcBlock::new(move |error: *mut NSError| {
        if let Some(sender) = sender.lock().unwrap().take() {
            let _ = sender.send(error.is_null());
        }
    });
    center.addNotificationRequest_withCompletionHandler(&request, Some(&completion));
    let accepted = receiver
        .recv_timeout(SUBMIT_WAIT)
        .map_err(|error| unavailable(format!("等待 macOS 通知提交超时：{error}")))?;
    if !accepted {
        return Err(unavailable("macOS 拒绝了系统通知请求".into()));
    }
    Ok(())
}

#[cfg(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd"
))]
fn show_system(app: &AppHandle, title: &str, body: &str) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    let app_id = {
        let exe = tauri::utils::platform::current_exe()
            .map_err(|error| unavailable(format!("无法确认 Windows 应用路径：{error}")))?;
        let exe_dir = exe
            .parent()
            .ok_or_else(|| unavailable("无法确认 Windows 应用目录".into()))?;
        windows_app_id(exe_dir, &app.config().identifier)
    };
    #[cfg(not(target_os = "windows"))]
    let app_id = {
        let _ = app;
        None
    };
    desktop::submit(title, body, app_id)
        .map_err(|error| unavailable(format!("系统通知提交失败：{error}")))
}

#[cfg(any(target_os = "windows", test))]
fn windows_app_id<'a>(exe_dir: &std::path::Path, identifier: &'a str) -> Option<&'a str> {
    // Development binaries have no installed Start Menu shortcut for this AUMID.
    if exe_dir.ends_with(std::path::Path::new("target").join("debug"))
        || exe_dir.ends_with(std::path::Path::new("target").join("release"))
    {
        None
    } else {
        Some(identifier)
    }
}

fn denied() -> AppError {
    AppError {
        code: "plugin_system_notification_denied",
        message: "系统通知未获允许，请在系统设置中允许 ZNet Sink 发送通知".into(),
        details: None,
    }
}

fn unavailable(message: String) -> AppError {
    AppError {
        code: "plugin_system_notification_unavailable",
        message,
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_successful_notification_posts_trigger_host_delivery() {
        let mut delivered = 0;
        let value = serde_json::json!({"message":"new announcement"});
        let result = deliver_if_post(Method::NotificationPost, value.clone(), |_| {
            delivered += 1;
            Ok(())
        });
        assert_eq!(result.unwrap(), value);
        assert_eq!(delivered, 1);

        deliver_if_post(Method::LogWrite, value.clone(), |_| {
            delivered += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(delivered, 1);

        let error = deliver_if_post(Method::NotificationPost, value, |_| {
            Err(AppError::internal("system notification failed"))
        });
        assert!(error.is_err());
    }

    #[test]
    fn windows_notification_identity_is_only_used_for_installed_binaries() {
        use std::path::Path;

        assert_eq!(
            windows_app_id(Path::new("target/debug"), "org.zerodenet.znetsink"),
            None
        );
        assert_eq!(
            windows_app_id(Path::new("target/release"), "org.zerodenet.znetsink"),
            None
        );
        assert_eq!(
            windows_app_id(Path::new("ZNet Sink"), "org.zerodenet.znetsink"),
            Some("org.zerodenet.znetsink")
        );
    }
}
