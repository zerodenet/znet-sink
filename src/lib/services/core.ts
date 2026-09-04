import { invoke } from '@tauri-apps/api/core';
import { warning } from './toast.svelte';
import type { CoreProcessStatus, CoreCallResult, CoreEndpoint, CoreEventSubscription, CoreConfigSnapshot, CoreConfigExportResult, CoreIpcOptions, AppError, CoreKernelInfo } from '$lib/types/core';
import type { AppConfig, AppConfigPatch, KernelSettingsExportResult } from '$lib/types/app-config';
import type { LogEntry, LogAppend, LogPage, LogQuery } from '$lib/types/logs';
import type { GuiCapabilitySnapshot, InteractionSurfaceSnapshot } from '$lib/types/capability';
import type { ClientCoreSnapshot, NodeScreenSnapshot, ProbeJobSnapshot, StartProbeRequest, ConfigProxyNode, SelfTestSnapshot, ConnectionStatus, ProxyModeStatus, CoreOverview, TrafficStats, PolicyGroup, PolicyOutbound, ProxyMode, GuiCoreHealth, GuiZeroCapabilities, GuiFeatureStatus, GuiFakeIpClearInput, GuiFakeIpClearResult, GuiPolicySelectionResult, GuiTargetProbeResult, GuiConnectionList, GuiConnectionItem, GuiConnectionCloseResult, ConfigPlanApplyResult } from '$lib/types/gui-api';
import type { DnsCacheResult, DnsLookupResult, FakeIpLookupResult, TraceRouteResult } from '$lib/types/diagnostics';
import type { DnsSettingsInput } from '$lib/types/dns';

export type { CoreProcessStatus, CoreCallResult, CoreEndpoint, CoreEventSubscription, CoreConfigSnapshot, CoreConfigExportResult, CoreIpcOptions, AppError, CoreKernelInfo, GuiCapabilitySnapshot, InteractionSurfaceSnapshot };

export interface AppErrorInfo {
  code?: string;
  message: string;
  fieldPath?: string;
  diagnostics: string[];
  cause?: string;
}

export function getAppErrorInfo(error: unknown, fallbackMessage: string): AppErrorInfo {
  const appError = error as { code?: string; message?: string; details?: unknown };
  if (appError.code === 'mode_restricted') {
    return {
      code: appError.code,
      message: `该功能仅在专业模式下可用：${appError.message || fallbackMessage}`,
      diagnostics: [],
    };
  }
  const envelope = appError.details && typeof appError.details === 'object'
    ? appError.details as Record<string, unknown>
    : undefined;
  const coreError = envelope?.error && typeof envelope.error === 'object'
    ? envelope.error as Record<string, unknown>
    : undefined;
  const code = typeof coreError?.code === 'string' ? coreError.code : appError.code;
  const message = appError.message || (typeof coreError?.message === 'string' ? coreError.message : fallbackMessage);
  const diagnostics = Array.isArray(coreError?.details)
    ? coreError.details
        .filter((detail): detail is Record<string, unknown> => !!detail && typeof detail === 'object')
        .map((detail) => {
          const detailMessage = typeof detail.message === 'string' ? detail.message : '';
          const fieldPath = typeof detail.field_path === 'string' ? detail.field_path : undefined;
          return fieldPath ? `${fieldPath}：${detailMessage}` : detailMessage;
        })
        .filter(Boolean)
    : [];
  const fieldPath = typeof coreError?.field_path === 'string' ? coreError.field_path : undefined;
  const cause = typeof coreError?.cause === 'string' ? coreError.cause : undefined;
  return { code, message, fieldPath, diagnostics, cause };
}

export function getAppErrorMessage(error: unknown, fallbackMessage: string): string {
  const info = getAppErrorInfo(error, fallbackMessage);
  return [info.code ? `[${info.code}] ${info.message}` : info.message, info.fieldPath ? `字段：${info.fieldPath}` : '', ...info.diagnostics, info.cause ? `原因：${info.cause}` : '']
    .filter(Boolean)
    .join('；');
}

export function handleAppError(error: unknown, fallbackMessage: string): void {
  warning(getAppErrorMessage(error, fallbackMessage));
}

// Core process lifecycle

export async function getClientCoreSnapshot(): Promise<ClientCoreSnapshot> {
  return invoke('gui_client_core_snapshot');
}

export async function getNodeScreenSnapshot(reason?: string): Promise<NodeScreenSnapshot> {
  return invoke('gui_node_screen_snapshot', { reason });
}

export async function startProbeJob(request: StartProbeRequest): Promise<ProbeJobSnapshot> {
  return invoke('gui_probe_job_start', { request });
}

export async function getProbeJob(jobId: number): Promise<ProbeJobSnapshot> {
  return invoke('gui_probe_job_get', { jobId });
}

export async function listProbeJobs(profileId?: string): Promise<ProbeJobSnapshot[]> {
  return invoke('gui_probe_job_list', { profileId });
}

export async function cancelProbeJob(jobId: number): Promise<ProbeJobSnapshot> {
  return invoke('gui_probe_job_cancel', { jobId });
}

export async function getCoreProcessStatus(): Promise<CoreProcessStatus> {
  return invoke('core_process_status');
}

export async function startCoreProcess(): Promise<import('$lib/types/core').CoreProcessTransitionResult> {
  return invoke('core_process_start');
}

export async function restartCoreProcess(): Promise<import('$lib/types/core').CoreProcessTransitionResult> {
  return invoke('core_process_restart');
}

// Core IPC

export async function getCoreStatus(options?: CoreIpcOptions): Promise<CoreCallResult> {
  return invoke('core_status', { options });
}

export async function pingCore(options?: CoreIpcOptions): Promise<CoreCallResult> {
  return invoke('core_ipc_ping', { options });
}

export async function queryCore(request: unknown, options?: CoreIpcOptions): Promise<CoreCallResult> {
  return invoke('core_ipc_query', { request, options });
}

export async function commandCore(method: string, params?: unknown, options?: CoreIpcOptions): Promise<CoreCallResult> {
  return invoke('core_ipc_command', { method, params, options });
}

export async function getCapabilities(options?: CoreIpcOptions): Promise<Record<string, unknown>> {
  return invoke('core_get_capabilities', { options });
}

export async function getCoreHealth(options?: CoreIpcOptions): Promise<Record<string, unknown>> {
  return invoke('core_get_health', { options });
}

export async function getCoreConfig(options?: CoreIpcOptions): Promise<Record<string, unknown>> {
  return invoke('core_get_config', { options });
}

export async function getCoreRuntime(options?: CoreIpcOptions): Promise<Record<string, unknown>> {
  return invoke('core_get_runtime', { options });
}

export async function getCoreStats(options?: CoreIpcOptions): Promise<Record<string, unknown>> {
  return invoke('core_get_stats', { options });
}

export async function getCorePolicies(options?: CoreIpcOptions): Promise<unknown> {
  return invoke('core_get_policies', { options });
}

// Policies

export async function getPolicies(options?: CoreIpcOptions): Promise<unknown> {
  return invoke('core_get_policies', { options });
}

export async function selectPolicy(policyTag: string, targetTag: string, options?: CoreIpcOptions): Promise<CoreCallResult> {
  return invoke('core_select_policy', { policyTag, targetTag, options });
}

export async function probePolicy(policyTag: string, options?: CoreIpcOptions): Promise<CoreCallResult> {
  return invoke('core_probe_policy', { policyTag, options });
}

// Flows

export interface FlowInfo {
  flowId: string;
  source: string;
  destination: string;
  protocol: string;
  bytesUp: number;
  bytesDown: number;
  startedAtUnixMs: number;
}

export async function queryFlows(): Promise<FlowInfo[]> {
  const result = await invoke<CoreCallResult>('core_ipc_query', {
    request: { active_flows: { limit: 100, filter: {} } },
    options: undefined,
  });
  if (!result.available || !result.response) return [];
  return parseFlows(result.response);
}

function parseFlows(data: unknown): FlowInfo[] {
  if (!data || typeof data !== 'object') return [];

  const obj = data as Record<string, unknown>;
  // Try known container keys
  let arr: unknown[] = [];
  for (const key of ['active_flows', 'activeFlows', 'flows', 'connections', 'data', 'items', 'result']) {
    const val = obj[key];
    if (Array.isArray(val)) { arr = val; break; }
  }
  // If no array found, check if the response itself is an array
  if (arr.length === 0 && Array.isArray(data)) {
    arr = data as unknown[];
  }

  return arr.map(parseSingleFlow).filter((f): f is FlowInfo => f !== null);
}

function parseSingleFlow(item: unknown): FlowInfo | null {
  if (!item || typeof item !== 'object') return null;
  const obj = item as Record<string, unknown>;

  const flowId = obj['flow_id'] || obj['flowId'] || obj['id'] || obj['connection_id'] || obj['connectionId'];
  if (!flowId || typeof flowId !== 'string') return null;

  const host = (obj['host'] || obj['destination'] || obj['dest'] || obj['remote'] || obj['addr'] || obj['address'] || '');
  const port = obj['port'] || obj['dest_port'] || obj['destPort'] || obj['remote_port'] || obj['remotePort'];

  return {
    flowId: flowId,
    source: typeof obj['source'] === 'string' ? obj['source'] : '-',
    destination: typeof host === 'string'
      ? host + (typeof port === 'number' ? `:${port}` : '')
      : '-',
    protocol: typeof obj['protocol'] === 'string' ? obj['protocol'] : typeof obj['type'] === 'string' ? obj['type'] : 'tcp',
    bytesUp: typeof obj['bytes_up'] === 'number' ? obj['bytes_up'] : typeof obj['bytesUp'] === 'number' ? obj['bytesUp'] : typeof obj['tx'] === 'number' ? obj['tx'] : 0,
    bytesDown: typeof obj['bytes_down'] === 'number' ? obj['bytes_down'] : typeof obj['bytesDown'] === 'number' ? obj['bytesDown'] : typeof obj['rx'] === 'number' ? obj['rx'] : 0,
    startedAtUnixMs: typeof obj['started_at'] === 'number' ? obj['started_at'] : typeof obj['startedAt'] === 'number' ? obj['startedAt'] : typeof obj['created_at'] === 'number' ? obj['created_at'] : Date.now(),
  };
}

export async function closeFlow(flowId: string, options?: CoreIpcOptions): Promise<CoreCallResult> {
  return invoke('core_close_flow', { flowId, options });
}

// Core events

export async function startCoreEvents(events?: string[], options?: CoreIpcOptions): Promise<CoreEventSubscription> {
  return invoke('core_events_start', { events, options });
}

export async function stopCoreEvents(): Promise<number> {
  return invoke('core_events_stop');
}

export async function startGuiEvents(events?: string[], options?: CoreIpcOptions): Promise<CoreEventSubscription> {
  return invoke('gui_events_start', { events, options });
}

export async function stopGuiEvents(): Promise<number> {
  return invoke('gui_events_stop');
}

// Core config

export async function getCoreConfigSnapshot(): Promise<CoreKernelInfo> {
  return invoke('core_config_get');
}

export async function exportActiveCoreConfig(): Promise<CoreConfigExportResult> {
  return invoke('core_config_export_active');
}

export interface CoreDownloadResult {
  success: boolean;
  executablePath: string;
  version?: string;
  message: string;
}

export async function downloadLatestCore(installDir?: string): Promise<CoreDownloadResult> {
  return invoke('core_download_latest', { installDir });
}

// App config

export async function getAppConfig(): Promise<AppConfig> {
  return invoke('app_config_get');
}

export async function updateAppConfig(patch: AppConfigPatch): Promise<AppConfig> {
  return invoke('app_config_update', { patch });
}

export async function applyTunSettings(tun: NonNullable<AppConfigPatch['tun']>): Promise<AppConfig> {
  return invoke('app_config_apply_tun', { tun });
}

export async function exportClientKernelSettings(path: string): Promise<KernelSettingsExportResult> {
  return invoke('app_config_export_kernel_settings', { path });
}

export async function importClientKernelSettings(path: string): Promise<AppConfig> {
  return invoke('app_config_import_kernel_settings', { path });
}

// Logs

export async function getLogs(query?: LogQuery): Promise<LogPage> {
  return invoke('logs_list', { query });
}

export async function appendLog(input: LogAppend): Promise<LogEntry> {
  return invoke('logs_append', { input });
}

export async function clearLogs(): Promise<void> {
  return invoke('logs_clear');
}

// GUI capabilities snapshot

export async function getGuiCapabilitiesSnapshot(): Promise<GuiCapabilitySnapshot> {
  return invoke('gui_capabilities_snapshot');
}

export async function getGuiInteractionSurfaceSnapshot(): Promise<InteractionSurfaceSnapshot> {
  return invoke('gui_interaction_surface_snapshot');
}

// System proxy

export interface SystemProxyStatus {
  enabled: boolean;
  host: string;
  port: number;
  socksEnabled?: boolean;
}

export async function enableSystemProxy(): Promise<SystemProxyStatus> {
  return invoke('system_proxy_enable');
}

export async function disableSystemProxy(): Promise<SystemProxyStatus> {
  return invoke('system_proxy_disable');
}

export async function getSystemProxyStatus(): Promise<SystemProxyStatus> {
  return invoke('system_proxy_status');
}

// GUI runtime snapshots backed by dedicated gui_* commands.
// These functions expose GUI-facing data instead of raw core_ipc_* results.

export async function getGuiSelfTestSnapshot(): Promise<SelfTestSnapshot> {
  return invoke('gui_self_test_snapshot');
}

export async function getGuiConnectionStatus(): Promise<ConnectionStatus> {
  const raw = await invoke<Record<string, unknown>>('gui_connection_status');
  return mapConnectionStatus(raw);
}

export async function guiConnect(): Promise<ConnectionStatus> {
  const raw = await invoke<Record<string, unknown>>('gui_connect');
  return mapConnectionStatus(raw);
}

export async function guiDisconnect(): Promise<ConnectionStatus> {
  const raw = await invoke<Record<string, unknown>>('gui_disconnect');
  return mapConnectionStatus(raw);
}

export async function getGuiProxyModeStatus(): Promise<ProxyModeStatus> {
  const raw = await invoke<Record<string, unknown>>('gui_proxy_mode_status');
  return mapProxyModeStatus(raw);
}

export async function guiSetProxyMode(mode: ProxyMode, restartCore: boolean = false): Promise<ProxyModeStatus> {
  const raw = await invoke<Record<string, unknown>>('gui_set_proxy_mode', { input: { mode, restartCore } });
  return mapProxyModeStatus(raw);
}

export async function getGuiCoreOverview(): Promise<CoreOverview> {
  const raw = await invoke<Record<string, unknown>>('gui_core_overview');
  return mapCoreOverview(raw);
}

export async function getGuiTrafficStats(): Promise<TrafficStats> {
  const raw = await invoke<Record<string, unknown>>('gui_traffic_snapshot');
  return mapTrafficStats(raw);
}

export async function getConfigProxyNodes(): Promise<ConfigProxyNode[]> {
  return invoke<ConfigProxyNode[]>('gui_proxy_nodes');
}

export async function getConfigPolicyGroups(): Promise<PolicyGroup[]> {
  const raw = await invoke<Record<string, unknown>[]>('gui_config_policy_groups');
  return mapPolicyGroups(raw);
}

export async function getGuiPolicyGroups(): Promise<PolicyGroup[]> {
  const raw = await invoke<Record<string, unknown>[]>('gui_policy_groups');
  return mapPolicyGroups(raw);
}

// Core health

export async function getGuiCoreHealth(): Promise<GuiCoreHealth> {
  return invoke('gui_core_health');
}

// Zero capabilities

export async function getGuiZeroCapabilities(): Promise<GuiZeroCapabilities> {
  return invoke('gui_zero_capabilities');
}

// Policy selection

export async function guiSelectPolicy(policyTag: string, targetTag: string): Promise<GuiPolicySelectionResult> {
  return invoke('gui_select_policy', { policyTag, targetTag });
}

export async function guiProbeTarget(targetTag: string): Promise<GuiTargetProbeResult> {
  return invoke('gui_probe_target', { targetTag });
}

// Feature status (TUN / DNS / Rules)

export async function getGuiTunStatus(): Promise<GuiFeatureStatus> {
  return invoke('gui_tun_status');
}

export async function enableGuiTun(): Promise<GuiFeatureStatus> {
  return invoke('gui_tun_enable');
}

export async function disableGuiTun(): Promise<GuiFeatureStatus> {
  return invoke('gui_tun_disable');
}

export async function getGuiDnsStatus(): Promise<GuiFeatureStatus> {
  return invoke('gui_dns_status');
}

export async function getGuiStackStatus(): Promise<GuiFeatureStatus> {
  return invoke('gui_stack_status');
}

export async function getGuiRuleStatus(): Promise<GuiFeatureStatus> {
  return invoke('gui_rule_status');
}

// GUI connections (Pro mode)

export interface GuiConnectionListOptions {
  limit?: number;
  inboundTag?: string;
  principalKey?: string;
}

export async function getGuiConnections(options?: GuiConnectionListOptions): Promise<GuiConnectionList> {
  return invoke('gui_connections', { options });
}

export async function getGuiConnectionDetail(flowId: string): Promise<GuiConnectionItem> {
  return invoke('gui_connection_detail', { flowId });
}

export async function guiCloseConnection(flowId: string): Promise<GuiConnectionCloseResult> {
  return invoke('gui_close_connection', { flowId });
}

export async function getGuiRecentConnections(options?: GuiConnectionListOptions): Promise<GuiConnectionList> {
  return invoke('gui_recent_connections', { options });
}

// Config hot-reload

export async function guiApplyConfig(config: Record<string, unknown>): Promise<unknown> {
  return invoke('gui_apply_config', { config });
}

export async function guiApplyDnsConfig(input: DnsSettingsInput): Promise<unknown> {
  return invoke('gui_apply_dns_config', { input });
}

export async function guiValidateConfig(config: Record<string, unknown>): Promise<unknown> {
  return invoke('gui_validate_config', { config });
}

export async function guiValidateDnsConfig(input: DnsSettingsInput): Promise<unknown> {
  return invoke('gui_validate_dns_config', { input });
}

export interface EffectiveConfigSource {
  id: string;
  label: string;
  paths: string[];
  enabled: boolean;
  count?: number;
}

export interface DnsEffectiveConfigInspection {
  activeProfileName?: string;
  baseConfig?: Record<string, unknown>;
  effectiveConfig?: Record<string, unknown>;
  sources: EffectiveConfigSource[];
  reason?: string;
}

export async function guiInspectDnsEffectiveConfig(
  input: DnsSettingsInput,
): Promise<DnsEffectiveConfigInspection> {
  return invoke('gui_inspect_dns_effective_config', { input });
}

/** Compatibility-only API. The current Zero IPC contract does not expose
 * config.plan_apply, and the Tauri command is intentionally not registered. */
export async function guiPlanApplyConfig(config: Record<string, unknown>): Promise<ConfigPlanApplyResult> {
  return invoke('gui_plan_apply_config', { config });
}

// Mode hot-switch

export async function guiSetMode(mode: string, outbound?: string): Promise<unknown> {
  return invoke('gui_set_mode', { mode, outbound });
}

// Policy probe

export async function guiProbePolicy(policyTag: string): Promise<import('$lib/types/gui-api').PolicyProbeAccepted> {
  const raw = await invoke<Record<string, unknown>>('gui_probe_policy', { policyTag });
  const result = objectFrom(raw, ['result']);
  return {
    accepted: boolFrom(raw, ['accepted']) ?? false,
    result: Object.keys(result).length > 0 ? {
      policyTag: stringFrom(result, ['policyTag', 'policy_tag']),
      probeTriggered: boolFrom(result, ['probeTriggered', 'probe_triggered']),
    } : undefined,
  };
}

// System tray status sync

/**
 * Push the current kernel / proxy state to the system-tray icon so the
 * tooltip, status summary, and proxy controls stay in sync without the user
 * opening the window. Best-effort no-op outside Tauri.
 */
export async function trayUpdateStatus(
  running: boolean,
  systemProxyEnabled: boolean,
  tunEnabled: boolean,
): Promise<void> {
  return invoke('tray_update_status', { running, systemProxyEnabled, tunEnabled });
}

// Network probe

export interface NetworkProbeResult {
  ip: string;
  country?: string;
  region?: string;
  city?: string;
  org?: string;
  isp?: string;
}

/**
 * Detect the host machine's current public IP and geo information.
 * Runs in the Tauri backend without calling the managed kernel and follows
 * the host's current system network/proxy configuration.
 */
export async function guiNetworkProbe(): Promise<NetworkProbeResult> {
  return invoke('gui_network_probe');
}

// Log paths

export interface GuiLogPaths {
  dataDir: string;
  logsDir: string;
  logFile: string;
  coreLogFile: string;
}

/**
 * Get the GUI log file path and directory.
 */
export async function guiLogPaths(): Promise<GuiLogPaths> {
  return invoke('gui_log_paths');
}

export interface GuiDiagnosticExport {
  directory: string;
  files: string[];
  createdAtUnixMs: number;
}

export interface GuiDebugStorageSummary {
  liveLogBytes: number;
  liveLogFileCount: number;
  diagnosticExportBytes: number;
  diagnosticExportCount: number;
  totalBytes: number;
}

export interface GuiDebugStorageCleanupFailure {
  target: string;
  message: string;
}

export interface GuiDebugStorageCleanupResult {
  bytesReclaimed: number;
  clearedFileCount: number;
  removedDiagnosticExportCount: number;
  remainingBytes: number;
  failures: GuiDebugStorageCleanupFailure[];
}

export async function guiExportDiagnostics(): Promise<GuiDiagnosticExport> {
  return invoke('gui_export_diagnostics');
}

export async function guiDebugStorageSummary(): Promise<GuiDebugStorageSummary> {
  return invoke('gui_debug_storage_summary');
}

export async function guiClearDebugStorage(): Promise<GuiDebugStorageCleanupResult> {
  return invoke('gui_clear_debug_storage');
}

// Debug

import type { DebugFramePage, DebugFrameQuery } from '$lib/types/debug';

export async function getGuiDebugFrames(query?: DebugFrameQuery): Promise<DebugFramePage> {
  const raw = await invoke<DebugFramePage | import('$lib/types/debug').DebugFrame[]>('gui_debug_frames', { query });
  if (!Array.isArray(raw)) return raw;

  // Compatibility with a Tauri backend that has not restarted since the
  // debug API moved from a frame array to a paginated response.
  const matching = raw
    .filter((frame) => !query?.frameType || frame.frameType === query.frameType)
    .sort((a, b) => a.id - b.id);
  const oldestAvailableId = matching[0]?.id;
  const before = query?.beforeId ?? Number.POSITIVE_INFINITY;
  const eligible = matching.filter((frame) => frame.id < before);
  const limit = query?.limit ?? 200;
  const items = eligible.slice(Math.max(0, eligible.length - limit));
  return {
    items,
    hasMore: items.length > 0 && oldestAvailableId != null && items[0].id > oldestAvailableId,
    oldestAvailableId,
  };
}

export async function clearDebugFrames(): Promise<void> {
  return invoke('gui_debug_clear');
}

// Diagnostics

export async function guiDnsLookup(hostname: string): Promise<DnsLookupResult> {
  return invoke<DnsLookupResult>('gui_dns_lookup', { hostname });
}

export async function guiDnsCache(domain?: string, limit?: number): Promise<DnsCacheResult> {
  return invoke<DnsCacheResult>('gui_dns_cache', { domain, limit });
}

export async function guiFakeIpLookup(input: { domain?: string; ip?: string }): Promise<FakeIpLookupResult> {
  return invoke<FakeIpLookupResult>('gui_fakeip_lookup', input);
}

export async function guiClearFakeIp(input?: GuiFakeIpClearInput): Promise<GuiFakeIpClearResult> {
  return invoke<GuiFakeIpClearResult>('gui_clear_fake_ip', { input });
}

export async function guiTraceRoute(
  target: string,
  port?: number,
  protocol?: string,
  inboundTag?: string,
): Promise<TraceRouteResult> {
  return invoke<TraceRouteResult>('gui_trace_route', { target, port, protocol, inboundTag });
}

export async function getGuiSinks(): Promise<unknown> {
  return invoke('gui_sinks');
}

export async function getGuiDiagnostics(): Promise<unknown> {
  return invoke('gui_diagnostics');
}

function mapConnectionStatus(raw: Record<string, unknown>): ConnectionStatus {
  const connected = boolFrom(raw, ['connected']) ?? false;
  const stage = stringFrom(raw, ['stage']) ?? 'disconnected';
  const coreAvailable = boolFrom(raw, ['core_available', 'coreAvailable']) ?? connected;
  const process = objectFrom(raw, ['process']);
  const stats = objectFrom(raw, ['stats']);
  const systemProxy = objectFrom(raw, ['system_proxy', 'systemProxy']);

  return {
    state: connected ? 'connected' : stageToState(stage),
    message: stringFrom(raw, ['last_error', 'lastError', 'message']) ?? (connected ? undefined : stage),
    uptimeMs: uptimeFromProcess(process),
    startedAtUnixMs: numberFrom(process, ['started_at_unix_ms', 'startedAtUnixMs']),
    activeConnections: numberFrom(stats, ['active_sessions', 'activeSessions']) ?? 0,
    coreAvailable,
    systemProxyEnabled: boolFrom(systemProxy, ['enabled']) ?? false,
    processState: stringFrom(process, ['state']),
    processPid: numberFrom(process, ['pid']) ?? null,
    processExitCode: numberFrom(process, ['exit_code', 'exitCode']) ?? null,
    processExitReason: stringFrom(process, ['exit_reason', 'exitReason']),
    processEndpointPath: stringFrom(process, ['endpoint_path', 'endpointPath']),
    localProxyHost: stringFrom(raw, ['local_proxy_host', 'localProxyHost']),
    localProxyPort: numberFrom(raw, ['local_proxy_port', 'localProxyPort']),
  };
}

function mapProxyModeStatus(raw: Record<string, unknown>): ProxyModeStatus {
  return {
    currentMode: (stringFrom(raw, ['mode']) as ProxyMode) ?? 'rule',
    availableModes: ['global', 'rule', 'direct'],
    message: stringFrom(raw, ['reason']),
  };
}

function mapCoreOverview(raw: Record<string, unknown>): CoreOverview {
  const process = objectFrom(raw, ['process']);
  const health = objectFrom(raw, ['health']);
  const stats = objectFrom(raw, ['stats']);
  const available = boolFrom(raw, ['available']) ?? false;

  return {
    coreState: available ? 'running' : processStateToCoreState(stringFrom(process, ['state'])),
    version: stringFrom(health, ['engine_version', 'engineVersion', 'version']),
    uptimeMs: uptimeFromProcess(process),
    memoryUsageBytes: numberFrom(stats, ['memory_usage_bytes', 'memoryUsageBytes']),
    cpuUsagePercent: numberFrom(stats, ['cpu_usage_percent', 'cpuUsagePercent']),
  };
}

function mapTrafficStats(raw: Record<string, unknown>): TrafficStats {
  const totals = objectFrom(raw, ['totals']);
  const rates = objectFrom(raw, ['rates']);

  return {
    uploadBytesPerSec: numberFrom(rates, ['upload_bps', 'uploadBps']) ?? 0,
    downloadBytesPerSec: numberFrom(rates, ['download_bps', 'downloadBps']) ?? 0,
    totalUploadBytes: numberFrom(totals, ['bytes_up', 'bytesUp']) ?? 0,
    totalDownloadBytes: numberFrom(totals, ['bytes_down', 'bytesDown']) ?? 0,
    connectionCount: numberFrom(totals, ['active_sessions', 'activeSessions']) ?? 0,
  };
}

function mapPolicyGroups(raw: Record<string, unknown>[]): PolicyGroup[] {
  return raw.map((group) => {
    const members = valuesFromContainer(group, ['outbounds', 'members', 'targets', 'children', 'proxies', 'items']);
    return {
      name: stringFrom(group, ['tag', 'policy_tag', 'policyTag', 'name', 'id']) ?? 'unknown',
      selected: stringFrom(group, ['selected', 'current', 'now', 'target']),
      kind: stringFrom(group, ['kind', 'type', 'policy_kind', 'policyKind']),
      outbounds: members
        .reduce((acc: PolicyOutbound[], member) => {
          if (!member || typeof member !== 'object') return acc;
          const item = member as Record<string, unknown>;
          const tag = stringFrom(item, ['tag', 'target_tag', 'targetTag', 'name', 'id', 'target']);
          if (!tag) return acc;
          acc.push({
            tag,
            type: stringFrom(item, ['kind', 'type', 'protocol']) ?? 'unknown',
            delayMs: numberFrom(item, ['delayMs', 'delay_ms', 'latency', 'latencyMs', 'latency_ms']),
            alive: boolFrom(item, ['alive', 'healthy', 'available']),
            lastCheckedUnixMs: numberFrom(item, ['lastCheckedUnixMs', 'last_checked_unix_ms']),
            lastError: stringFrom(item, ['lastError', 'last_error', 'error']),
          });
          return acc;
        }, []),
    };
  });
}

function valuesFromContainer(value: Record<string, unknown>, keys: string[]): unknown[] {
  for (const key of keys) {
    const candidate = value[key];
    if (Array.isArray(candidate)) return candidate;
    if (candidate && typeof candidate === 'object') return Object.values(candidate as Record<string, unknown>);
  }
  return [];
}

function objectFrom(value: Record<string, unknown>, keys: string[]): Record<string, unknown> {
  for (const key of keys) {
    const candidate = value[key];
    if (candidate && typeof candidate === 'object' && !Array.isArray(candidate)) {
      return candidate as Record<string, unknown>;
    }
  }
  return {};
}

function stringFrom(value: Record<string, unknown>, keys: string[]): string | undefined {
  for (const key of keys) {
    const candidate = value[key];
    if (typeof candidate === 'string' && candidate.trim()) return candidate.trim();
  }
  return undefined;
}

function numberFrom(value: Record<string, unknown>, keys: string[]): number | undefined {
  for (const key of keys) {
    const candidate = value[key];
    if (typeof candidate === 'number' && Number.isFinite(candidate)) return candidate;
    if (typeof candidate === 'string') {
      const parsed = Number(candidate);
      if (Number.isFinite(parsed)) return parsed;
    }
  }
  return undefined;
}

function boolFrom(value: Record<string, unknown>, keys: string[]): boolean | undefined {
  for (const key of keys) {
    const candidate = value[key];
    if (typeof candidate === 'boolean') return candidate;
    if (typeof candidate === 'string') {
      const normalized = candidate.toLowerCase();
      if (['true', '1', 'yes'].includes(normalized)) return true;
      if (['false', '0', 'no'].includes(normalized)) return false;
    }
  }
  return undefined;
}

function uptimeFromProcess(process: Record<string, unknown>): number | undefined {
  const startedAt = numberFrom(process, ['started_at_unix_ms', 'startedAtUnixMs']);
  return startedAt ? Math.max(0, Date.now() - startedAt) : undefined;
}

function processStateToCoreState(state?: string): CoreOverview['coreState'] {
  switch (state) {
    case 'running':
      return 'running';
    case 'starting':
      return 'starting';
    case 'stopping':
      return 'stopping';
    case 'failed':
      return 'error';
    case 'exited':
      return 'stopped';
    default:
      return 'stopped';
  }
}

function stageToState(stage: string): ConnectionStatus['state'] {
  switch (stage) {
    case 'connected':
      return 'connected';
    case 'connecting':
      return 'connecting';
    case 'failed':
      return 'error';
    default:
      return 'disconnected';
  }
}
