use std::process::Child;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::thread::JoinHandle;

use crate::kernel::zero::adapter::TrafficSample;
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
    gui_event_generation: Arc<AtomicU64>,
    next_record_id: AtomicU64,
    app_config: Mutex<AppConfig>,
    proxy_configs: Mutex<Vec<ProxyConfigProfile>>,
    subscriptions: Mutex<Vec<SubscriptionProfile>>,
    rule_sets: Mutex<Vec<RuleSetProfile>>,
    logs: Mutex<Vec<LogEntry>>,
    traffic_sample: Mutex<Option<TrafficSample>>,
    core_process: Mutex<ManagedCoreProcess>,
    zero_features_cache: Mutex<Option<ZeroFeaturesCache>>,
}

#[derive(Clone, Debug)]
pub(crate) struct ZeroFeaturesCache {
    pub features: Vec<String>,
    pub cached_at_unix_ms: u64,
}

pub(crate) struct ManagedCoreProcess {
    pub child: Option<Child>,
    pub stderr_handle: Option<JoinHandle<()>>,
    pub status: CoreProcessStatus,
}

impl Drop for ManagedCoreProcess {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            eprintln!("[ZNet] shutdown: killing core process (pid={})", child.id());
            let _ = child.kill();
            let _ = child.wait();
        }
        self.stderr_handle.take().map(|h| h.join());
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new(AppConfig::default())
    }
}

impl AppState {
    pub(crate) fn new(app_config: AppConfig) -> Self {
        Self::with_domain_data(app_config, Vec::new(), Vec::new(), Vec::new(), Vec::new())
    }

    pub(crate) fn with_domain_data(
        app_config: AppConfig,
        proxy_configs: Vec<ProxyConfigProfile>,
        subscriptions: Vec<SubscriptionProfile>,
        rule_sets: Vec<RuleSetProfile>,
        logs: Vec<LogEntry>,
    ) -> Self {
        let next_record_id = logs.iter().map(|entry| entry.id).max().unwrap_or(0);
        let proxy_configs = normalize_proxy_configs(proxy_configs);

        Self {
            core_event_generation: Arc::new(AtomicU64::default()),
            gui_event_generation: Arc::new(AtomicU64::default()),
            next_record_id: AtomicU64::new(next_record_id),
            app_config: Mutex::new(app_config),
            proxy_configs: Mutex::new(proxy_configs),
            subscriptions: Mutex::new(subscriptions),
            rule_sets: Mutex::new(rule_sets),
            logs: Mutex::new(logs),
            traffic_sample: Mutex::new(None),
            core_process: Mutex::new(ManagedCoreProcess::default()),
            zero_features_cache: Mutex::new(None),
        }
    }

    pub(crate) fn next_core_event_generation(&self) -> u64 {
        self.core_event_generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub(crate) fn core_event_generation(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.core_event_generation)
    }

    pub(crate) fn next_gui_event_generation(&self) -> u64 {
        self.gui_event_generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub(crate) fn gui_event_generation(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.gui_event_generation)
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

    pub(crate) fn traffic_sample(&self) -> &Mutex<Option<TrafficSample>> {
        &self.traffic_sample
    }

    pub(crate) fn core_process(&self) -> &Mutex<ManagedCoreProcess> {
        &self.core_process
    }

    pub(crate) fn zero_features_cache(&self) -> &Mutex<Option<ZeroFeaturesCache>> {
        &self.zero_features_cache
    }
}

#[cfg(test)]
mod tests {
    use super::AppState;
    use crate::models::{
        app_config::AppConfig,
        logs::{LogEntry, LogLevel, LogSource},
        proxy_config::{ProxyConfigCapabilities, ProxyConfigProfile},
    };

    #[test]
    fn next_record_id_continues_after_loaded_logs() {
        let state = AppState::with_domain_data(
            AppConfig::default(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![
                LogEntry {
                    id: 2,
                    source: LogSource::App,
                    level: LogLevel::Info,
                    message: "a".to_string(),
                    fields: None,
                    occurred_at_unix_ms: 1,
                },
                LogEntry {
                    id: 7,
                    source: LogSource::Core,
                    level: LogLevel::Error,
                    message: "b".to_string(),
                    fields: None,
                    occurred_at_unix_ms: 2,
                },
            ],
        );

        assert_eq!(state.next_record_id(), 8);
        assert_eq!(state.next_record_id(), 9);
    }

    #[test]
    fn loaded_proxy_configs_keep_only_one_active() {
        let state = AppState::with_domain_data(
            AppConfig::default(),
            vec![
                proxy_profile("a", false),
                proxy_profile("b", true),
                proxy_profile("c", true),
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        let profiles = state.proxy_configs().lock().unwrap();
        assert!(!profiles[0].active);
        assert!(profiles[1].active);
        assert!(!profiles[2].active);
    }

    #[test]
    fn loaded_proxy_configs_promote_first_when_none_active() {
        let state = AppState::with_domain_data(
            AppConfig::default(),
            vec![proxy_profile("a", false), proxy_profile("b", false)],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        let profiles = state.proxy_configs().lock().unwrap();
        assert!(profiles[0].active);
        assert!(!profiles[1].active);
    }

    fn proxy_profile(id: &str, active: bool) -> ProxyConfigProfile {
        ProxyConfigProfile {
            id: id.to_string(),
            name: id.to_string(),
            kernel: "zero".to_string(),
            format: "json".to_string(),
            path: None,
            content: None,
            active,
            updated_at_unix_ms: 1,
            capabilities: ProxyConfigCapabilities::default(),
        }
    }
}

fn normalize_proxy_configs(mut profiles: Vec<ProxyConfigProfile>) -> Vec<ProxyConfigProfile> {
    let mut active_index = None;
    for (index, profile) in profiles.iter_mut().enumerate() {
        if profile.active {
            if active_index.is_none() {
                active_index = Some(index);
            } else {
                profile.active = false;
            }
        }
    }

    if !profiles.is_empty() && active_index.is_none() {
        profiles[0].active = true;
    }

    profiles
}

impl Default for ManagedCoreProcess {
    fn default() -> Self {
        Self {
            child: None,
            stderr_handle: None,
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
                exit_reason: None,
                last_error: None,
            },
        }
    }
}
