//! Concrete lifecycle hook implementations for each startup phase.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::errors::AppResult;
use crate::models::app_config::AppConfig;
use crate::services::domain_store::DomainStoreData;
use crate::services::{app_config_store, domain_store, log_store, system_proxy_guard};

use super::{Lifecycle, Phase, OnPhase};

// ── Data built during Config phase, consumed by State phase ──

/// Accumulated data from the Config phase, shared with later phases.
pub struct StartupData {
    pub config_path: PathBuf,
    pub app_config: AppConfig,
    pub domain_data: DomainStoreData,
    pub logs: Vec<crate::models::logs::LogEntry>,
}

// ── Guard phase ──

pub struct GuardPhase;

impl OnPhase for GuardPhase {
    fn phase(&self) -> Phase {
        Phase::Guard
    }
    fn name(&self) -> &str {
        "crash_guard"
    }
    fn run(&self) -> AppResult<()> {
        system_proxy_guard::install_panic_hook();
        system_proxy_guard::cleanup_on_startup();
        Ok(())
    }
}

// ── Config phase ──

/// Loads app config, domain data, and logs from disk.
/// Stores results into a shared `StartupData` for later phases.
pub struct ConfigPhase {
    pub data: Arc<Mutex<Option<StartupData>>>,
}

impl OnPhase for ConfigPhase {
    fn phase(&self) -> Phase {
        Phase::Config
    }
    fn name(&self) -> &str {
        "load_config"
    }
    fn run(&self) -> AppResult<()> {
        let config_path = app_config_store::default_config_path().unwrap_or_else(|e| {
            crate::services::logs::znet_log(None, crate::models::logs::LogLevel::Warn, format!("failed to resolve app config path: {e:?}, using fallback"));
            PathBuf::from("app-config.json")
        });
        let app_config = app_config_store::load_or_default(&config_path).unwrap_or_else(|e| {
            crate::services::logs::znet_log(None, crate::models::logs::LogLevel::Warn, format!("failed to load app config: {e:?}, using defaults"));
            AppConfig::default()
        });
        let domain_data = domain_store::load_all().unwrap_or_else(|e| {
            crate::services::logs::znet_log(None, crate::models::logs::LogLevel::Warn, format!("failed to load domain data: {e:?}, using empty data"));
            DomainStoreData::default()
        });
        let logs = log_store::load_recent(app_config.logs.max_entries).unwrap_or_else(|e| {
            crate::services::logs::znet_log(None, crate::models::logs::LogLevel::Warn, format!("failed to load logs: {e:?}, using empty log buffer"));
            Vec::new()
        });
        if let Err(e) = log_store::rotate(app_config.logs.max_entries) {
            crate::services::logs::znet_log(None, crate::models::logs::LogLevel::Warn, format!("failed to rotate logs: {e:?}"));
        }

        *self.data.lock().expect("startup data lock") = Some(StartupData {
            config_path,
            app_config,
            domain_data,
            logs,
        });
        Ok(())
    }
}

// ── Builder helpers ──

/// Construct a fully-wired [`Lifecycle`] with all built-in hooks.
/// Returns the lifecycle and a shared handle to the startup data.
pub fn build_builtin() -> (Lifecycle, Arc<Mutex<Option<StartupData>>>) {
    let mut lc = Lifecycle::new();
    let startup_data = Arc::new(Mutex::new(None));

    // Phase 1: Guard — crash protection
    lc.add_hook(Box::new(GuardPhase));

    // Phase 2: Config — load from disk
    lc.add_hook(Box::new(ConfigPhase {
        data: Arc::clone(&startup_data),
    }));

    // Phase 3–5 (State, Register, Runtime) are handled inside the Tauri builder
    // because they need Tauri-specific types (AppHandle, etc.).
    // The lifecycle orchestrates their ordering through shutdown coordination.

    (lc, startup_data)
}
