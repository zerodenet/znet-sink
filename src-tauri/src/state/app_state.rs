use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::process::Child;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::thread::JoinHandle;

use crate::client_core::{
    ClientCore, ClientCoreError, ClientCoreSnapshot, ClientScope, ProbeJobId, ProbeJobSnapshot,
    ProbeObservation, ProbeTargetResult, ProfileId, SourceStatus, StartProbeOutcome,
    StartProbeRequest,
};
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
    client_core: Mutex<ClientCore>,
    core_event_generation: Arc<AtomicU64>,
    gui_event_generation: Arc<AtomicU64>,
    core_process_monitor_generation: Arc<AtomicU64>,
    next_record_id: AtomicU64,
    app_config: Mutex<AppConfig>,
    proxy_configs: Mutex<Vec<ProxyConfigProfile>>,
    subscriptions: Mutex<Vec<SubscriptionProfile>>,
    rule_sets: Mutex<Vec<RuleSetProfile>>,
    proxy_config_operation: tokio::sync::Mutex<()>,
    subscription_syncs: Mutex<HashSet<String>>,
    rule_set_updates: Mutex<HashSet<String>>,
    logs: Mutex<Vec<LogEntry>>,
    traffic_sample: Mutex<Option<TrafficSample>>,
    core_process: Mutex<ManagedCoreProcess>,
    zero_features_cache: Mutex<Option<ZeroFeaturesCache>>,
    /// Set to `true` the moment shutdown begins. Long-lived background
    /// tasks (core-process watchdog, event streams) poll this to stop
    /// restarting/reconnecting instead of fighting the shutdown sequence.
    shutting_down: Arc<AtomicBool>,
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
            eprintln!(
                "[ZNet] shutdown: closing core lifetime pipe (pid={})",
                child.id()
            );
            child.stdin.take();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if std::time::Instant::now() < deadline => {
                        std::thread::sleep(std::time::Duration::from_millis(25));
                    }
                    _ => {
                        // This is still scoped to the exact child owned by
                        // this state; never kill processes by executable name.
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                }
            }
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
        let active_profile = proxy_configs.iter().find(|profile| profile.active);
        let active_profile_id = active_profile.map(|profile| ProfileId(profile.id.clone()));
        let config_revision = config_revision(active_profile);

        Self {
            client_core: Mutex::new(ClientCore::new(active_profile_id, config_revision)),
            core_event_generation: Arc::new(AtomicU64::default()),
            gui_event_generation: Arc::new(AtomicU64::default()),
            core_process_monitor_generation: Arc::new(AtomicU64::default()),
            next_record_id: AtomicU64::new(next_record_id),
            app_config: Mutex::new(app_config),
            proxy_configs: Mutex::new(proxy_configs),
            subscriptions: Mutex::new(subscriptions),
            rule_sets: Mutex::new(rule_sets),
            proxy_config_operation: tokio::sync::Mutex::new(()),
            subscription_syncs: Mutex::new(HashSet::new()),
            rule_set_updates: Mutex::new(HashSet::new()),
            logs: Mutex::new(logs),
            traffic_sample: Mutex::new(None),
            core_process: Mutex::new(ManagedCoreProcess::default()),
            zero_features_cache: Mutex::new(None),
            shutting_down: Arc::new(AtomicBool::new(false)),
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

    pub(crate) fn client_core_snapshot(&self) -> ClientCoreSnapshot {
        self.client_core
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .snapshot()
    }

    pub(crate) fn client_core_configuration_committed(
        &self,
        active_profile: Option<&ProxyConfigProfile>,
    ) {
        self.client_core
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .configuration_committed(
                active_profile.map(|profile| ProfileId(profile.id.clone())),
                config_revision(active_profile),
                crate::services::common::now_unix_ms(),
            );
    }

    pub(crate) fn client_core_instance_started(&self) {
        self.client_core
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .core_instance_started(crate::services::common::now_unix_ms());
    }

    pub(crate) fn client_core_instance_lost(&self) {
        self.client_core
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .core_instance_lost(crate::services::common::now_unix_ms());
    }

    pub(crate) fn set_client_core_source_status(&self, status: SourceStatus) {
        self.client_core
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .set_source_status(status);
    }

    pub(crate) fn start_client_probe(
        &self,
        request: StartProbeRequest,
    ) -> Result<StartProbeOutcome, ClientCoreError> {
        self.client_core
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .start_probe(request, crate::services::common::now_unix_ms())
    }

    pub(crate) fn record_client_probe_result(
        &self,
        id: ProbeJobId,
        scope: &ClientScope,
        result: ProbeTargetResult,
    ) -> Option<ProbeJobSnapshot> {
        let (updated, observations) = {
            let mut core = self
                .client_core
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let updated = core.record_probe_result(id, scope, result);
            let observations = updated.is_some().then(|| core.observations(None));
            (updated, observations)
        };
        if let Some(observations) = observations {
            let _ = crate::services::probe_history::save(&observations);
        }
        updated
    }

    pub(crate) fn record_client_probe_observation(&self, observation: ProbeObservation) -> bool {
        let (recorded, observations) = {
            let mut core = self
                .client_core
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let recorded = core.record_observation(observation);
            let observations = recorded.then(|| core.observations(None));
            (recorded, observations)
        };
        if let Some(observations) = observations {
            let _ = crate::services::probe_history::save(&observations);
        }
        recorded
    }

    pub(crate) fn client_probe_observations_for_config(
        &self,
        scope: &ClientScope,
    ) -> Vec<ProbeObservation> {
        self.client_core
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .observations_for_config(scope)
    }

    pub(crate) fn restore_client_probe_observations(&self, observations: Vec<ProbeObservation>) {
        self.client_core
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .restore_observations(observations);
    }

    pub(crate) fn get_client_probe_job(&self, id: ProbeJobId) -> Option<ProbeJobSnapshot> {
        self.client_core
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_probe_job(id)
    }

    pub(crate) fn list_client_probe_jobs(
        &self,
        profile_id: Option<String>,
    ) -> Vec<ProbeJobSnapshot> {
        let profile_id = profile_id.map(ProfileId);
        self.client_core
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .list_probe_jobs(profile_id.as_ref())
    }

    pub(crate) fn cancel_client_probe(&self, id: ProbeJobId) -> Option<ProbeJobSnapshot> {
        self.client_core
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .cancel_probe(id, crate::services::common::now_unix_ms())
    }

    pub(crate) fn timeout_client_probe(&self, id: ProbeJobId) -> Option<ProbeJobSnapshot> {
        let (updated, observations) = {
            let mut core = self
                .client_core
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let updated = core.timeout_probe(id, crate::services::common::now_unix_ms());
            let observations = updated
                .as_ref()
                .is_some_and(|job| job.state == crate::client_core::ProbeJobState::TimedOut)
                .then(|| core.observations(None));
            (updated, observations)
        };
        if let Some(observations) = observations {
            let _ = crate::services::probe_history::save(&observations);
        }
        updated
    }

    pub(crate) fn next_core_process_monitor_generation(&self) -> u64 {
        self.core_process_monitor_generation
            .fetch_add(1, Ordering::SeqCst)
            + 1
    }

    pub(crate) fn core_process_monitor_generation(&self) -> u64 {
        self.core_process_monitor_generation.load(Ordering::SeqCst)
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

    pub(crate) fn proxy_config_operation(&self) -> &tokio::sync::Mutex<()> {
        &self.proxy_config_operation
    }

    pub(crate) fn subscription_syncs(&self) -> &Mutex<HashSet<String>> {
        &self.subscription_syncs
    }

    pub(crate) fn rule_set_updates(&self) -> &Mutex<HashSet<String>> {
        &self.rule_set_updates
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

    /// Returns `true` once shutdown has begun.
    pub(crate) fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    /// Cloneable handle to the shutdown flag, so shutdown callbacks
    /// registered in `lib.rs` (which can't borrow `AppState`) can flip it.
    pub(crate) fn shutting_down_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutting_down)
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

fn config_revision(profile: Option<&ProxyConfigProfile>) -> crate::client_core::ConfigRevision {
    let Some(profile) = profile else {
        return crate::client_core::ConfigRevision(0);
    };
    let bytes = serde_json::to_vec(&(
        &profile.kernel,
        &profile.format,
        &profile.path,
        &profile.content,
    ))
    .unwrap_or_default();
    let digest = Sha256::digest(bytes);
    crate::client_core::ConfigRevision(u64::from_be_bytes(
        digest[..8].try_into().expect("sha256 prefix length"),
    ))
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

    #[test]
    fn core_process_monitor_generation_advances_monotonically() {
        let state = AppState::default();

        assert_eq!(state.core_process_monitor_generation(), 0);
        assert_eq!(state.next_core_process_monitor_generation(), 1);
        assert_eq!(state.core_process_monitor_generation(), 1);
        assert_eq!(state.next_core_process_monitor_generation(), 2);
        assert_eq!(state.core_process_monitor_generation(), 2);
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
