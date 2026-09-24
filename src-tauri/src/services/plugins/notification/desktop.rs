//! Windows toast and freedesktop notification submission.

pub(super) fn submit(
    title: &str,
    body: &str,
    app_id: Option<&str>,
) -> Result<(), notify_rust::error::Error> {
    let mut notification = notify_rust::Notification::new();
    notification.summary(title).body(body).auto_icon();
    #[cfg(target_os = "windows")]
    if let Some(app_id) = app_id {
        notification.app_id(app_id);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = app_id;
    notification.show().map(|_| ())
}
