use std::process::Child;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use crate::models::{
    app_config::AppConfig,
    core_process::{CoreProcessState, CoreProcessStatus},
    logs::LogEntry,
    proxy_config::ProxyConfigProfile,
    rule_set::RuleSetProfile,
    subscription::SubscriptionProfile,
};

pub struct AppState {
    core_event_generation: Arc<AtomicU64>,
    next_record_id: AtomicU64,
    app_config: Mutex<AppConfig>,
    proxy_configs: Mutex<Vec<ProxyConfigProfile>>,
    subscriptions: Mutex<Vec<SubscriptionProfile>>,
    rule_sets: Mutex<Vec<RuleSetProfile>>,
    logs: Mutex<Vec<LogEntry>>,
    core_process: Mutex<ManagedCoreProcess>,
}

pub(crate) struct ManagedCoreProcess {
    pub child: Option<Child>,
    pub status: CoreProcessStatus,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new(AppConfig::default())
    }
}

impl AppState {
    pub(crate) fn new(app_config: AppConfig) -> Self {
        Self::with_domain_data(app_config, Vec::new(), Vec::new(), Vec::new())
    }

    pub(crate) fn with_domain_data(
        app_config: AppConfig,
        proxy_configs: Vec<ProxyConfigProfile>,
        subscriptions: Vec<SubscriptionProfile>,
        rule_sets: Vec<RuleSetProfile>,
    ) -> Self {
        Self {
            core_event_generation: Arc::new(AtomicU64::default()),
            next_record_id: AtomicU64::default(),
            app_config: Mutex::new(app_config),
            proxy_configs: Mutex::new(proxy_configs),
            subscriptions: Mutex::new(subscriptions),
            rule_sets: Mutex::new(rule_sets),
            logs: Mutex::default(),
            core_process: Mutex::new(ManagedCoreProcess::default()),
        }
    }

    pub(crate) fn next_core_event_generation(&self) -> u64 {
        self.core_event_generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub(crate) fn core_event_generation(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.core_event_generation)
    }

    pub(crate) fn next_record_id(&self) -> u64 {
        self.next_record_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub(crate) fn app_config(&self) -> &Mutex<AppConfig> {
        &self.app_config
    }

    pub(crate) fn proxy_configs(&self) -> &Mutex<Vec<ProxyConfigProfile>> {
        &self.proxy_configs
    }

    pub(crate) fn subscriptions(&self) -> &Mutex<Vec<SubscriptionProfile>> {
        &self.subscriptions
    }

    pub(crate) fn rule_sets(&self) -> &Mutex<Vec<RuleSetProfile>> {
        &self.rule_sets
    }

    pub(crate) fn logs(&self) -> &Mutex<Vec<LogEntry>> {
        &self.logs
    }

    pub(crate) fn core_process(&self) -> &Mutex<ManagedCoreProcess> {
        &self.core_process
    }
}

impl Default for ManagedCoreProcess {
    fn default() -> Self {
        Self {
            child: None,
            status: CoreProcessStatus {
                state: CoreProcessState::NotStarted,
                pid: None,
                kernel: "zero".to_string(),
                executable_path: None,
                working_dir: None,
                config_path: None,
                endpoint_path: String::new(),
                started_at_unix_ms: None,
                exited_at_unix_ms: None,
                exit_code: None,
                last_error: None,
            },
        }
    }
}
