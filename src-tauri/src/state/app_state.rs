#[cfg(feature = "tool-node-probe")]
use crate::client_core::{
    ClientCoreError, ProbeJobId, ProbeTargetResult, StartProbeOutcome, StartProbeRequest,
};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};

use crate::client_core::{
    ClientCore, ClientCoreSnapshot, ClientScope, ProbeJobSnapshot, ProbeObservation, ProfileId,
    SourceStatus,
};
use crate::kernel::zero::adapter::TrafficSample;
use crate::models::{
    app_config::AppConfig, logs::LogEntry, proxy_config::ProxyConfigProfile,
    rule_set::RuleSetProfile, subscription::SubscriptionProfile,
};

pub struct AppState {
    /// Distinguishes host lifetimes even when local scope counters restart at 1.
    /// Missing secure entropy disables recovery admission; no weak fallback.
    host_session_id: Option<String>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    plugins: crate::services::plugins::Host,
    #[cfg(any(feature = "tool-dns", feature = "tool-route"))]
    tool_runtime: crate::services::tool_jobs::ToolRuntime,
    #[cfg(feature = "tool-node-probe")]
    probe_runtime: crate::services::probe::runtime::ProbeRuntime,
    client_core: Mutex<ClientCore>,
    capabilities: znet_client_core::capability::Manager,
    core_event_generation: Arc<AtomicU64>,
    observations: znet_engine_client::SubscriptionOwner,
    next_record_id: AtomicU64,
    app_config: Mutex<AppConfig>,
    proxy_configs: Mutex<Vec<ProxyConfigProfile>>,
    subscriptions: Mutex<Vec<SubscriptionProfile>>,
    rule_sets: Mutex<Vec<RuleSetProfile>>,
    configuration: crate::configuration::Workspace,
    endpoint_restoration: crate::services::endpoint_restoration::EndpointRestoration,
    diagnostic_exports: crate::services::diagnostics::DiagnosticExports,
    mobile_subscription_scheduler:
        crate::services::mobile_subscription_scheduler::MobileSubscriptionScheduler,
    runtime_bridge: znet_engine_client::bridge::RuntimeBridge,
    flow_closes: crate::services::flow_close::Owner,
    route_editor: crate::configuration::route_editor::Owner,
    runtime_operation: tokio::sync::Mutex<()>,
    subscription_syncs: Mutex<HashSet<String>>,
    rule_set_updates: Mutex<HashSet<String>>,
    logs: Mutex<Vec<LogEntry>>,
    traffic_sample: Mutex<Option<TrafficSample>>,
    runtime_host: crate::runtime_host::Host,
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

impl Default for AppState {
    fn default() -> Self {
        Self::new(AppConfig::default())
    }
}

impl AppState {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub(crate) fn plugins(&self) -> &crate::services::plugins::Host {
        &self.plugins
    }
    pub(crate) fn capabilities(&self) -> &znet_client_core::capability::Manager {
        &self.capabilities
    }

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
            host_session_id: new_host_session_id(),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            plugins: crate::services::plugins::Host::default(),
            #[cfg(any(feature = "tool-dns", feature = "tool-route"))]
            tool_runtime: crate::services::tool_jobs::ToolRuntime::default(),
            #[cfg(feature = "tool-node-probe")]
            probe_runtime: crate::services::probe::runtime::ProbeRuntime::default(),
            capabilities: znet_client_core::capability::Manager::default(),
            client_core: Mutex::new(ClientCore::new(active_profile_id, config_revision)),
            core_event_generation: Arc::new(AtomicU64::default()),
            observations: znet_engine_client::SubscriptionOwner::default(),
            next_record_id: AtomicU64::new(next_record_id),
            app_config: Mutex::new(app_config),
            proxy_configs: Mutex::new(proxy_configs),
            subscriptions: Mutex::new(subscriptions),
            rule_sets: Mutex::new(rule_sets),
            configuration: crate::configuration::Workspace::default(),
            endpoint_restoration:
                crate::services::endpoint_restoration::EndpointRestoration::default(),
            diagnostic_exports: crate::services::diagnostics::DiagnosticExports::default(),
            mobile_subscription_scheduler:
                crate::services::mobile_subscription_scheduler::MobileSubscriptionScheduler::default(
                ),
            runtime_bridge: znet_engine_client::bridge::RuntimeBridge::default(),
            flow_closes: crate::services::flow_close::Owner::default(),
            route_editor: crate::configuration::route_editor::Owner::default(),
            runtime_operation: tokio::sync::Mutex::new(()),
            subscription_syncs: Mutex::new(HashSet::new()),
            rule_set_updates: Mutex::new(HashSet::new()),
            logs: Mutex::new(logs),
            traffic_sample: Mutex::new(None),
            runtime_host: crate::runtime_host::Host::default(),
            zero_features_cache: Mutex::new(None),
            shutting_down: Arc::new(AtomicBool::new(false)),
        }
    }

    #[cfg(any(feature = "tool-dns", feature = "tool-route"))]
    pub(crate) fn tool_runtime(&self) -> &crate::services::tool_jobs::ToolRuntime {
        &self.tool_runtime
    }

    #[cfg(feature = "tool-node-probe")]
    pub(crate) fn probe_runtime(&self) -> &crate::services::probe::runtime::ProbeRuntime {
        &self.probe_runtime
    }

    pub(crate) fn next_core_event_generation(&self) -> u64 {
        self.core_event_generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Recovery may inspect records from another session, but must explicitly
    /// bind a new review to this session before publishing any local material.
    pub(crate) fn host_session_id(&self) -> crate::errors::AppResult<&str> {
        self.host_session_id.as_deref().ok_or_else(|| {
            crate::errors::AppError::internal(
                "无法建立安全宿主会话身份；恢复控制暂不可用，请保留原操作记录",
            )
        })
    }

    pub(crate) fn core_event_generation(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.core_event_generation)
    }

    pub(crate) fn observations(&self) -> &znet_engine_client::SubscriptionOwner {
        &self.observations
    }

    pub(crate) fn endpoint_restoration(
        &self,
    ) -> &crate::services::endpoint_restoration::EndpointRestoration {
        &self.endpoint_restoration
    }

    pub(crate) fn client_core_snapshot(&self) -> ClientCoreSnapshot {
        let core = self.client_core.lock().unwrap_or_else(|e| e.into_inner());
        #[allow(unused_mut)]
        let mut snapshot = core.snapshot();
        #[cfg(feature = "tool-node-probe")]
        {
            snapshot.active_probe_jobs = self
                .probe_runtime
                .jobs
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .list_probe_jobs(None)
                .into_iter()
                .filter(|j| !j.state.is_terminal())
                .collect();
        }
        snapshot
    }

    #[cfg(any(feature = "tool-dns", feature = "tool-route"))]
    pub(crate) fn start_client_tool_job(
        &self,
        request: crate::models::tool_job::StartToolJobRequest,
    ) -> crate::errors::AppResult<crate::models::tool_job::ToolJobSnapshot> {
        let core = self
            .client_core
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let snapshot = core.snapshot();
        let scope = snapshot.scope;
        if request
            .expected_scope
            .as_ref()
            .is_some_and(|expected| expected != &scope)
            || request
                .expected_client_revision
                .is_some_and(|expected| expected != snapshot.revision.0)
        {
            return Err(crate::errors::AppError::conflict(
                "tool_job",
                "",
                "任务所属配置或内核实例已变化，请在当前配置重新发起",
            ));
        }
        // Same core -> tool-runtime lock order as invalidation. Admission and
        // insertion are one client coordination boundary, not a remote-core CAS.
        self.tool_runtime
            .start(scope, request, crate::services::common::now_unix_ms())
    }

    #[cfg(any(feature = "tool-dns", feature = "tool-route"))]
    pub(crate) fn cancel_client_tool_job(
        &self,
        id: crate::models::tool_job::ToolJobId,
        expected: Option<&ClientScope>,
        expected_revision: Option<u64>,
    ) -> crate::errors::AppResult<crate::models::tool_job::ToolJobSnapshot> {
        let core = self
            .client_core
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let snapshot = core.snapshot();
        let scope = snapshot.scope;
        let job = self
            .tool_runtime
            .get(id)
            .ok_or_else(|| crate::errors::AppError::not_found("tool_job", id.0.to_string()))?;
        if expected.is_some_and(|expected| expected != &scope)
            || job.scope != scope
            || expected_revision.is_some_and(|expected| expected != snapshot.revision.0)
        {
            return Err(crate::errors::AppError::conflict(
                "tool_job",
                id.0.to_string(),
                "历史任务不属于当前配置或实例，不能继续控制",
            ));
        }
        self.tool_runtime
            .cancel(id, crate::services::common::now_unix_ms())
            .ok_or_else(|| crate::errors::AppError::not_found("tool_job", id.0.to_string()))
    }

    pub(crate) fn diagnostic_exports(&self) -> &crate::services::diagnostics::DiagnosticExports {
        &self.diagnostic_exports
    }

    pub(crate) fn mobile_subscription_scheduler(
        &self,
    ) -> &crate::services::mobile_subscription_scheduler::MobileSubscriptionScheduler {
        &self.mobile_subscription_scheduler
    }

    pub(crate) fn runtime_bridge(&self) -> &znet_engine_client::bridge::RuntimeBridge {
        &self.runtime_bridge
    }

    pub(crate) fn flow_closes(&self) -> &crate::services::flow_close::Owner {
        &self.flow_closes
    }

    pub(crate) fn route_editor(&self) -> &crate::configuration::route_editor::Owner {
        &self.route_editor
    }

    pub(crate) fn commit_diagnostic_capture(
        &self,
        expected: &ClientScope,
        request: crate::models::debug::CaptureRequest,
    ) -> crate::errors::AppResult<crate::models::debug::CaptureStatus> {
        let core = self
            .client_core
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if &core.snapshot().scope != expected {
            return Err(crate::errors::AppError::conflict(
                "diagnostic_capture",
                "",
                "所属配置或内核实例已变化，请重新核对捕获范围",
            ));
        }
        // Same core -> capture lock order as scope invalidation, so a start
        // cannot escape a concurrent profile/instance transition.
        crate::models::debug::capture_request(request, expected.clone())
    }

    pub(crate) fn ensure_client_scope(
        &self,
        expected: Option<&ClientScope>,
    ) -> crate::errors::AppResult<()> {
        let core = self.client_core.lock().unwrap_or_else(|e| e.into_inner());
        if expected.is_some_and(|expected| expected != &core.snapshot().scope) {
            return Err(crate::errors::AppError::conflict(
                "client_scope",
                "",
                "所属配置或内核实例已变化，请刷新后重选",
            ));
        }
        Ok(())
    }

    /// Client lifecycle generation also rejects A -> B -> A when a content
    /// digest is reused. This is not a remote kernel transaction precondition.
    pub(crate) fn ensure_client_scope_generation(
        &self,
        expected: &ClientScope,
        generation: u64,
    ) -> crate::errors::AppResult<()> {
        let core = self
            .client_core
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let snapshot = core.snapshot();
        if &snapshot.scope != expected || snapshot.scope_generation.0 != generation {
            return Err(crate::errors::AppError::conflict(
                "client_scope",
                "",
                "配置或实例代次已变化，请重新预览原范围",
            ));
        }
        Ok(())
    }

    /// Fence a bounded synchronous local publication against lifecycle events.
    /// Acquire any profile/subscription/rule guards before this call. The
    /// callback may publish local storage and settings, but must not await,
    /// emit events, or call a method that locks client_core. Notify the client
    /// of a committed configuration only after this fence and data guards drop.
    pub(crate) fn with_client_scope_generation<T>(
        &self,
        expected: &ClientScope,
        generation: u64,
        publish: impl FnOnce() -> crate::errors::AppResult<T>,
    ) -> crate::errors::AppResult<T> {
        let core = self.client_core.lock().unwrap_or_else(|e| e.into_inner());
        let snapshot = core.snapshot();
        if &snapshot.scope != expected || snapshot.scope_generation.0 != generation {
            return Err(crate::errors::AppError::conflict(
                "client_scope",
                "",
                "配置或实例代次已变化，未发布本地材料；请保留草稿并重新核对",
            ));
        }
        let result = publish();
        drop(core);
        result
    }

    pub(crate) fn bind_runtime_bridge(
        &self,
        expected: &ClientScope,
        binding: znet_engine_client::bridge::RuntimeBinding,
    ) -> crate::errors::AppResult<()> {
        let core = self
            .client_core
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if &core.snapshot().scope != expected {
            return Err(crate::errors::AppError::conflict(
                "runtime_bridge",
                "",
                "所属配置或实例已变化，请重新核对",
            ));
        }
        // Scope transitions acquire the same core -> bridge lock order. A
        // discovery reply cannot rebind an invalidated old client scope.
        self.runtime_bridge
            .bind(binding)
            .map_err(|_| crate::errors::AppError::internal("控制通道身份不完整"))
    }

    pub(crate) fn client_core_configuration_committed(
        &self,
        active_profile: Option<&ProxyConfigProfile>,
    ) {
        let mut core = self.client_core.lock().unwrap_or_else(|e| e.into_inner());
        core.configuration_committed(
            active_profile.map(|profile| ProfileId(profile.id.clone())),
            config_revision(active_profile),
            crate::services::common::now_unix_ms(),
        );
        crate::models::debug::invalidate_capture_scope(&core.snapshot().scope);
        self.runtime_bridge.invalidate();
        #[cfg(feature = "tool-node-probe")]
        self.probe_runtime.invalidate(
            crate::client_core::ProbeJobState::InvalidatedByConfigChange,
            crate::services::common::now_unix_ms(),
        );
        #[cfg(feature = "tool-node-probe")]
        self.checkpoint_probe_jobs();
        #[cfg(any(feature = "tool-dns", feature = "tool-route"))]
        self.tool_runtime.invalidate(
            crate::models::tool_job::ToolJobState::InvalidatedByConfigChange,
            crate::services::common::now_unix_ms(),
        );
        #[cfg(any(feature = "tool-dns", feature = "tool-route"))]
        self.checkpoint_tool_jobs();
    }

    pub(crate) fn client_core_instance_started(&self) {
        let mut core = self.client_core.lock().unwrap_or_else(|e| e.into_inner());
        core.core_instance_started(crate::services::common::now_unix_ms());
        crate::models::debug::invalidate_capture_scope(&core.snapshot().scope);
        self.runtime_bridge.invalidate();
        #[cfg(feature = "tool-node-probe")]
        self.probe_runtime.invalidate(
            crate::client_core::ProbeJobState::InvalidatedByCoreRestart,
            crate::services::common::now_unix_ms(),
        );
        #[cfg(feature = "tool-node-probe")]
        self.checkpoint_probe_jobs();
        #[cfg(any(feature = "tool-dns", feature = "tool-route"))]
        self.tool_runtime.invalidate(
            crate::models::tool_job::ToolJobState::InvalidatedByCoreRestart,
            crate::services::common::now_unix_ms(),
        );
        #[cfg(any(feature = "tool-dns", feature = "tool-route"))]
        self.checkpoint_tool_jobs();
    }

    pub(crate) fn client_core_instance_lost(&self) {
        let mut core = self.client_core.lock().unwrap_or_else(|e| e.into_inner());
        core.core_instance_lost(crate::services::common::now_unix_ms());
        crate::models::debug::invalidate_capture_scope(&core.snapshot().scope);
        self.runtime_bridge.invalidate();
        #[cfg(feature = "tool-node-probe")]
        self.probe_runtime.invalidate(
            crate::client_core::ProbeJobState::InvalidatedByCoreRestart,
            crate::services::common::now_unix_ms(),
        );
        #[cfg(feature = "tool-node-probe")]
        self.checkpoint_probe_jobs();
        #[cfg(any(feature = "tool-dns", feature = "tool-route"))]
        self.tool_runtime.invalidate(
            crate::models::tool_job::ToolJobState::InvalidatedByCoreRestart,
            crate::services::common::now_unix_ms(),
        );
        #[cfg(any(feature = "tool-dns", feature = "tool-route"))]
        self.checkpoint_tool_jobs();
    }

    #[cfg(any(feature = "tool-dns", feature = "tool-route"))]
    fn checkpoint_tool_jobs(&self) {
        if let Err(error) = self.tool_runtime.checkpoint(&self.capabilities) {
            crate::services::file_logger::line(&format!(
                "tool jobs: failed to persist invalidation: {}",
                error.message
            ));
        }
    }

    pub(crate) fn set_client_core_source_status(&self, status: SourceStatus) {
        self.client_core
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .set_source_status(status);
    }

    #[cfg(all(feature = "tool-node-probe", test))]
    pub(crate) fn start_client_probe(
        &self,
        request: StartProbeRequest,
    ) -> Result<StartProbeOutcome, ClientCoreError> {
        self.start_client_probe_scoped(request, None)
    }

    #[cfg(feature = "tool-node-probe")]
    pub(crate) fn start_client_probe_scoped(
        &self,
        request: StartProbeRequest,
        expected_scope: Option<&ClientScope>,
    ) -> Result<StartProbeOutcome, ClientCoreError> {
        let mut core = self.client_core.lock().unwrap_or_else(|e| e.into_inner());
        let scope = core.snapshot().scope;
        if expected_scope.is_some_and(|expected| expected != &scope) {
            return Err(ClientCoreError {
                code: "stale_probe_scope".into(),
                message: "测速所属配置或内核实例已变化，请在当前配置重新发起".into(),
            });
        }
        let outcome =
            self.probe_runtime
                .start(&scope, request, crate::services::common::now_unix_ms())?;
        if outcome.created {
            core.advance_snapshot();
        }
        Ok(outcome)
    }

    #[cfg(feature = "tool-node-probe")]
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
            if &core.snapshot().scope != scope {
                return None;
            }
            let mut jobs = self
                .probe_runtime
                .jobs
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let updated = jobs.record_probe_result(id, scope, result);
            for observation in jobs.take_observations() {
                core.record_observation(observation);
            }
            if updated.is_some() {
                core.advance_snapshot();
            }
            let observations = updated.is_some().then(|| core.observations(None));
            (updated, observations)
        };
        if let Some(observations) = observations {
            let _ = crate::services::probe_history::save(&observations);
        }
        if updated.is_some() {
            self.checkpoint_probe_jobs();
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

    #[cfg(feature = "tool-node-probe")]
    pub(crate) fn get_client_probe_job(&self, id: ProbeJobId) -> Option<ProbeJobSnapshot> {
        self.probe_runtime
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_probe_job(id)
    }

    pub(crate) fn list_client_probe_jobs(
        &self,
        profile_id: Option<String>,
    ) -> Vec<ProbeJobSnapshot> {
        #[cfg(feature = "tool-node-probe")]
        {
            let profile_id = profile_id.map(ProfileId);
            self.probe_runtime
                .jobs
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .list_probe_jobs(profile_id.as_ref())
        }
        #[cfg(not(feature = "tool-node-probe"))]
        {
            let _ = profile_id;
            Vec::new()
        }
    }

    #[cfg(feature = "tool-node-probe")]
    pub(crate) fn cancel_client_probe(&self, id: ProbeJobId) -> Option<ProbeJobSnapshot> {
        let mut core = self.client_core.lock().unwrap_or_else(|e| e.into_inner());
        self.probe_runtime.cancel_execution(id);
        let updated = self
            .probe_runtime
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cancel_probe(id, crate::services::common::now_unix_ms());
        if updated.is_some() {
            core.advance_snapshot();
        }
        drop(core);
        if updated.is_some() {
            self.checkpoint_probe_jobs();
        }
        updated
    }

    #[cfg(feature = "tool-node-probe")]
    pub(crate) fn timeout_client_probe(&self, id: ProbeJobId) -> Option<ProbeJobSnapshot> {
        let (updated, observations) = {
            let mut core = self
                .client_core
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            self.probe_runtime.cancel_execution(id);
            let mut jobs = self
                .probe_runtime
                .jobs
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let updated = jobs.timeout_probe(id, crate::services::common::now_unix_ms());
            for observation in jobs.take_observations() {
                core.record_observation(observation);
            }
            if updated.is_some() {
                core.advance_snapshot();
            }
            let observations = updated
                .as_ref()
                .is_some_and(|job| job.state == crate::client_core::ProbeJobState::TimedOut)
                .then(|| core.observations(None));
            (updated, observations)
        };
        if let Some(observations) = observations {
            let _ = crate::services::probe_history::save(&observations);
        }
        if updated.is_some() {
            self.checkpoint_probe_jobs();
        }
        updated
    }

    #[cfg(feature = "tool-node-probe")]
    pub(crate) fn checkpoint_probe_jobs(&self) {
        if let Err(error) = self.probe_runtime.checkpoint(&self.capabilities) {
            crate::services::file_logger::line(&format!(
                "probe jobs: failed to persist transition: {}",
                error.message
            ));
        }
    }

    pub(crate) fn next_core_process_monitor_generation(&self) -> u64 {
        self.runtime_host.advance_generation()
    }

    pub(crate) fn core_process_monitor_generation(&self) -> u64 {
        self.runtime_host.generation()
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

    pub(crate) fn configuration(&self) -> &crate::configuration::Workspace {
        &self.configuration
    }

    pub(crate) fn proxy_config_operation(&self) -> &tokio::sync::Mutex<()> {
        &self.runtime_operation
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

    pub(crate) fn runtime_host(&self) -> &crate::runtime_host::Host {
        &self.runtime_host
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

fn new_host_session_id() -> Option<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).ok()?;
    Some(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
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

    profiles
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
    fn host_recreation_with_identical_local_counters_has_a_distinct_session() {
        let first = AppState::default();
        let second = AppState::default();
        assert_eq!(
            first.client_core_snapshot().scope,
            second.client_core_snapshot().scope
        );
        assert_eq!(
            first.client_core_snapshot().scope_generation,
            second.client_core_snapshot().scope_generation
        );
        assert_ne!(
            first.host_session_id().unwrap(),
            second.host_session_id().unwrap()
        );
        assert_eq!(first.host_session_id().unwrap().len(), 32);
    }

    #[test]
    fn missing_secure_host_identity_rejects_recovery_admission() {
        let mut state = AppState::default();
        state.host_session_id = None;
        assert!(state.host_session_id().is_err());
        let _ = state.client_core_snapshot();
    }

    #[test]
    fn concurrent_host_identity_reads_share_the_same_session() {
        let state = std::sync::Arc::new(AppState::default());
        let original = state.host_session_id().unwrap().to_owned();
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let state = std::sync::Arc::clone(&state);
                std::thread::spawn(move || state.host_session_id().unwrap().to_owned())
            })
            .collect();
        for thread in threads {
            assert_eq!(thread.join().unwrap(), original);
        }
    }

    #[test]
    fn host_session_child_process_probe() {
        let Some(output) = std::env::var_os("ZNET_TEST_HOST_SESSION_OUTPUT") else {
            return;
        };
        let state = AppState::default();
        let snapshot = state.client_core_snapshot();
        let metadata = serde_json::json!({
            "session": state.host_session_id().unwrap(),
            "scope": snapshot.scope,
            "generation": snapshot.scope_generation.0,
        });
        std::fs::write(output, serde_json::to_vec(&metadata).unwrap()).unwrap();
    }

    #[test]
    fn actual_host_process_restart_cannot_reuse_scope_counter_as_session_identity() {
        let directory = tempfile::tempdir().unwrap();
        let mut snapshots = Vec::new();
        for index in 0..2 {
            let output = directory.path().join(format!("owner-{index}.json"));
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "state::app_state::tests::host_session_child_process_probe",
                    "--test-threads=1",
                ])
                .env("ZNET_TEST_HOST_SESSION_OUTPUT", &output)
                .output()
                .unwrap();
            assert!(child.status.success(), "host identity child process failed");
            snapshots.push(
                serde_json::from_slice::<serde_json::Value>(&std::fs::read(output).unwrap())
                    .unwrap(),
            );
        }
        assert_eq!(snapshots[0]["scope"], snapshots[1]["scope"]);
        assert_eq!(snapshots[0]["generation"], snapshots[1]["generation"]);
        assert_ne!(snapshots[0]["session"], snapshots[1]["session"]);
    }

    #[test]
    fn next_record_id_continues_after_loaded_logs() {
        let state = AppState::with_domain_data(
            AppConfig::default(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![
                LogEntry {
                    diagnostic_event: None,
                    id: 2,
                    source: LogSource::App,
                    level: LogLevel::Info,
                    message: "a".to_string(),
                    fields: None,
                    occurred_at_unix_ms: 1,
                },
                LogEntry {
                    diagnostic_event: None,
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
    fn loaded_proxy_configs_preserve_explicitly_stopped_selection() {
        let state = AppState::with_domain_data(
            AppConfig::default(),
            vec![proxy_profile("a", false), proxy_profile("b", false)],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        let profiles = state.proxy_configs().lock().unwrap();
        assert!(!profiles[0].active);
        assert!(!profiles[1].active);
        assert!(state.client_core_snapshot().scope.profile_id.is_none());
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

    #[test]
    fn old_same_tag_configuration_scope_is_rejected_before_control() {
        let state = AppState::default();
        let old = state.client_core_snapshot().scope;
        assert!(state.ensure_client_scope(Some(&old)).is_ok());
        {
            let mut core = state.client_core.lock().unwrap();
            core.configuration_committed(
                Some(crate::client_core::ProfileId("new".into())),
                crate::client_core::ConfigRevision(7),
                1,
            );
        }
        assert!(state.ensure_client_scope(Some(&old)).is_err());
        let current = state.client_core_snapshot().scope;
        assert!(state.ensure_client_scope(Some(&current)).is_ok());
        state.client_core.lock().unwrap().core_instance_started(2);
        assert!(state.ensure_client_scope(Some(&current)).is_err());
        assert!(state.ensure_client_scope(None).is_ok());
    }

    #[cfg(feature = "tool-node-probe")]
    #[test]
    fn delayed_probe_targets_cannot_start_in_replacement_configuration() {
        let state = AppState::default();
        let old = state.client_core_snapshot().scope;
        state.client_core.lock().unwrap().configuration_committed(
            Some(crate::client_core::ProfileId("new".into())),
            crate::client_core::ConfigRevision(7),
            1,
        );
        let request = crate::client_core::StartProbeRequest {
            kind: crate::client_core::ProbeJobKind::Outbound,
            target_tags: vec!["same-tag".into()],
            timeout_ms: Some(30_000),
        };
        let error = state
            .start_client_probe_scoped(request, Some(&old))
            .unwrap_err();
        assert_eq!(error.code, "stale_probe_scope");
        assert!(state.list_client_probe_jobs(None).is_empty());
    }

    #[test]
    fn diagnostic_start_after_scope_change_is_rejected_before_global_capture() {
        let state = AppState::default();
        let old = state.client_core_snapshot().scope;
        state.client_core.lock().unwrap().core_instance_started(1);
        let request = serde_json::from_value(serde_json::json!({
            "action": "start", "durationSeconds": 1, "capacityMiB": 1
        }))
        .unwrap();
        let error = state.commit_diagnostic_capture(&old, request).unwrap_err();
        assert_eq!(error.code, "conflict");
    }

    #[cfg(any(feature = "tool-dns", feature = "tool-route"))]
    #[test]
    fn stale_tool_start_is_rejected_before_id_allocation_and_cannot_cancel_old_history() {
        use crate::models::tool_job::{StartToolJobRequest, ToolJobKind, ToolJobState};
        let state = AppState::default();
        let original = state.client_core_snapshot().scope;
        let request = |scope| StartToolJobRequest {
            expected_scope: Some(scope),
            expected_client_revision: None,
            kind: ToolJobKind::DnsCache,
            params: serde_json::json!({"limit": 50}),
            timeout_ms: None,
        };
        let first = state
            .start_client_tool_job(request(original.clone()))
            .unwrap();
        assert_eq!(first.id.0, 1);
        state.client_core.lock().unwrap().configuration_committed(
            Some(crate::client_core::ProfileId("replacement".into())),
            crate::client_core::ConfigRevision(7),
            1,
        );
        assert_eq!(
            state
                .start_client_tool_job(request(original.clone()))
                .unwrap_err()
                .code,
            "conflict"
        );
        assert_eq!(state.tool_runtime.list(None).len(), 1);
        let replacement = state.client_core_snapshot().scope;
        assert_eq!(
            state
                .cancel_client_tool_job(first.id, Some(&replacement), None)
                .unwrap_err()
                .code,
            "conflict"
        );
        assert_eq!(
            state.tool_runtime.get(first.id).unwrap().state,
            ToolJobState::Queued
        );
        let second = state
            .start_client_tool_job(request(replacement.clone()))
            .unwrap();
        assert_eq!(second.id.0, 2);
        assert_eq!(
            state
                .cancel_client_tool_job(second.id, Some(&replacement), None)
                .unwrap()
                .state,
            ToolJobState::Cancelled
        );
        state.client_core.lock().unwrap().core_instance_started(2);
        assert_eq!(
            state
                .start_client_tool_job(request(replacement.clone()))
                .unwrap_err()
                .code,
            "conflict"
        );
        assert_eq!(
            state
                .cancel_client_tool_job(second.id, Some(&replacement), None)
                .unwrap_err()
                .code,
            "conflict"
        );
    }

    #[cfg(any(feature = "tool-dns", feature = "tool-route"))]
    #[test]
    fn same_profile_tool_revision_change_rejects_old_request_without_rebinding() {
        use crate::models::tool_job::StartToolJobRequest;
        let state = AppState::default();
        let original = state.client_core_snapshot().scope;
        state.client_core.lock().unwrap().configuration_committed(
            original.profile_id.clone(),
            crate::client_core::ConfigRevision(u64::MAX),
            1,
        );
        let request: StartToolJobRequest = serde_json::from_value(serde_json::json!({
            "expectedScope": original, "kind":"dns_lookup", "params":{"hostname":"same.example"}
        }))
        .unwrap();
        assert_eq!(
            state.start_client_tool_job(request).unwrap_err().code,
            "conflict"
        );
        assert!(state.tool_runtime.list(None).is_empty());
        let current = state.client_core_snapshot().scope;
        let request: StartToolJobRequest = serde_json::from_value(serde_json::json!({
            "expectedScope": current, "kind":"dns_lookup", "params":{"hostname":"same.example"}
        }))
        .unwrap();
        assert_eq!(
            state
                .start_client_tool_job(request)
                .unwrap()
                .scope
                .config_revision,
            crate::client_core::ConfigRevision(u64::MAX)
        );
    }

    #[cfg(any(feature = "tool-dns", feature = "tool-route"))]
    #[test]
    fn tool_snapshot_revision_rejects_aba_before_queue_allocation_and_old_cancel() {
        use crate::models::tool_job::{StartToolJobRequest, ToolJobState};
        let state = AppState::default();
        let before = state.client_core_snapshot();
        {
            let mut core = state.client_core.lock().unwrap();
            core.configuration_committed(
                Some(crate::client_core::ProfileId("B".into())),
                crate::client_core::ConfigRevision(7),
                1,
            );
            core.configuration_committed(
                before.scope.profile_id.clone(),
                before.scope.config_revision,
                2,
            );
        }
        assert_eq!(state.client_core_snapshot().scope, before.scope);
        let stale: StartToolJobRequest = serde_json::from_value(serde_json::json!({
            "kind":"dns_cache", "params":{"limit":50}, "expectedScope":before.scope,
            "expectedClientRevision":before.revision,
        }))
        .unwrap();
        assert_eq!(
            state.start_client_tool_job(stale).unwrap_err().code,
            "conflict"
        );
        assert!(state.tool_runtime.list(None).is_empty());
        let current = state.client_core_snapshot();
        let fresh: StartToolJobRequest = serde_json::from_value(serde_json::json!({
            "kind":"dns_cache", "params":{"limit":50}, "expectedScope":current.scope,
            "expectedClientRevision":current.revision,
        }))
        .unwrap();
        let job = state.start_client_tool_job(fresh).unwrap();
        assert_eq!(job.id.0, 1);
        assert_eq!(
            state
                .cancel_client_tool_job(job.id, Some(&before.scope), Some(before.revision.0))
                .unwrap_err()
                .code,
            "conflict"
        );
        assert_eq!(
            state.tool_runtime.get(job.id).unwrap().state,
            ToolJobState::Queued
        );
    }

    #[test]
    fn local_publication_scope_fence_refuses_aba_before_any_write() {
        let state = AppState::default();
        let before = state.client_core_snapshot();
        state.client_core_configuration_committed(Some(&proxy_profile("other", true)));
        state.client_core_configuration_committed(None);
        assert_eq!(state.client_core_snapshot().scope, before.scope);
        let writes = std::cell::Cell::new(0);
        let result =
            state.with_client_scope_generation(&before.scope, before.scope_generation.0, || {
                writes.set(writes.get() + 1);
                Ok(())
            });
        assert_eq!(result.unwrap_err().code, "conflict");
        assert_eq!(writes.get(), 0);
    }

    #[test]
    fn local_publication_scope_fence_holds_lifecycle_events_until_publish_finishes() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        let state = AppState::default();
        let before = state.client_core_snapshot();
        let published = AtomicBool::new(false);
        let (attempted, attempted_rx) = mpsc::channel();
        let (completed, completed_rx) = mpsc::channel();
        std::thread::scope(|threads| {
            let mut worker = None;
            state
                .with_client_scope_generation(&before.scope, before.scope_generation.0, || {
                    // A deterministic assertion of the actual lifecycle lock,
                    // followed by a real lifecycle event on another thread.
                    assert!(state.client_core.try_lock().is_err());
                    worker = Some(threads.spawn(|| {
                        attempted.send(()).unwrap();
                        state.client_core_instance_started();
                        assert!(published.load(Ordering::SeqCst));
                        completed.send(()).unwrap();
                    }));
                    attempted_rx
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .unwrap();
                    assert!(matches!(
                        completed_rx.try_recv(),
                        Err(mpsc::TryRecvError::Empty)
                    ));
                    published.store(true, Ordering::SeqCst);
                    Ok(())
                })
                .unwrap();
            worker.unwrap().join().unwrap();
        });
        completed_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert_ne!(
            state.client_core_snapshot().scope_generation,
            before.scope_generation
        );
    }

    #[test]
    fn local_publication_scope_fence_preserves_failure_and_releases_lifecycle_lock() {
        let state = AppState::default();
        let before = state.client_core_snapshot();
        let result: crate::errors::AppResult<()> =
            state.with_client_scope_generation(&before.scope, before.scope_generation.0, || {
                Err(crate::errors::AppError::internal("publication failed"))
            });
        assert_eq!(result.unwrap_err().message, "publication failed");
        assert!(state.client_core.try_lock().is_ok());
        state.client_core_instance_started();
        assert_ne!(
            state.client_core_snapshot().scope_generation,
            before.scope_generation
        );
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
            managed_source: None,
            updated_at_unix_ms: 1,
            capabilities: ProxyConfigCapabilities::default(),
        }
    }
}
