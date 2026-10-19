use super::complete_final_exit;
use crate::{capture::shutdown::clear_saved_tun_intent, models::app_config::AppConfig};
use std::sync::atomic::{AtomicU8, Ordering};

#[test]
fn native_exit_without_request_persists_tun_off_before_returning() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app-config.json");
    let mut config = AppConfig::default();
    config.tun.enabled = Some(true);
    crate::services::app_config_store::save(&path, &config).unwrap();
    let stage = AtomicU8::new(0);
    let mut stopped = false;

    complete_final_exit(
        &tauri::RunEvent::Exit,
        &stage,
        || clear_saved_tun_intent(&mut config, &path).unwrap(),
        || stopped = true,
    );

    assert!(stopped);
    assert_eq!(stage.load(Ordering::SeqCst), 2);
    assert_eq!(
        crate::services::app_config_store::load_or_default(&path)
            .unwrap()
            .tun
            .enabled,
        Some(false)
    );
    complete_final_exit(
        &tauri::RunEvent::Exit,
        &stage,
        || panic!("must not persist twice"),
        || panic!("native final Exit must not repeat cleanup"),
    );
}

#[test]
fn completed_requested_exit_does_not_repeat_capture_cleanup() {
    complete_final_exit(
        &tauri::RunEvent::Exit,
        &AtomicU8::new(2),
        || panic!("must not persist twice"),
        || panic!("final Exit must not repeat the completed requested-exit cleanup"),
    );
}

#[test]
fn non_exit_events_do_not_clear_capture_intent() {
    let stage = AtomicU8::new(0);
    complete_final_exit(
        &tauri::RunEvent::Ready,
        &stage,
        || panic!("must not persist at startup"),
        || panic!("startup must not be treated as native termination"),
    );
    assert_eq!(stage.load(Ordering::SeqCst), 0);
}

#[test]
fn native_exit_during_requested_cleanup_persists_off_and_waits_without_replay() {
    let stage = std::sync::Arc::new(AtomicU8::new(1));
    let worker_stage = stage.clone();
    let worker = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        worker_stage.store(2, Ordering::SeqCst);
    });
    let mut persisted = false;
    complete_final_exit(
        &tauri::RunEvent::Exit,
        &stage,
        || persisted = true,
        || panic!("already running cleanup must not be replayed"),
    );
    assert!(persisted);
    assert_eq!(stage.load(Ordering::SeqCst), 2);
    worker.join().unwrap();
}
