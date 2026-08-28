// GUI 层业务接口类型定义
// 对应 Rust 后端的 gui_* 命令 DTO

export type SelfTestCheckStatus = 'pass' | 'warn' | 'fail';

export interface SelfTestCheckItem {
  key: string;
  status: SelfTestCheckStatus;
  message: string;
  details?: unknown;
}

export interface SelfTestSnapshot {
  ready: boolean;
  blockingIssues: string[];
  warningCount: number;
  activeProxyConfigId?: string;
  activeProxyConfigName?: string;
  checks: SelfTestCheckItem[];
  suggestedFlow: 'setup' | 'ready' | 'troubleshoot';
}

export type ConnectionState = 'disconnected' | 'connecting' | 'connected' | 'error';
export type ProxyMode = 'global' | 'rule' | 'direct';

export type ClientSourceStatus = 'initializing' | 'ready' | 'degraded' | 'offline';

export interface ClientScope {
  profileId?: string;
  configRevision: number;
  coreInstanceId: number;
}

/** Revisioned recovery point owned by the Rust Client Core. */
export interface ClientCoreSnapshot {
  revision: number;
  scope: ClientScope;
  sourceStatus: ClientSourceStatus;
  activeProbeJobs: ProbeJobSnapshot[];
}

export type ProbeJobKind = 'outbound' | 'manual_policy' | 'scheduled_policy_observation';
export type ProbeJobState =
  | 'running'
  | 'completed'
  | 'partially_failed'
  | 'failed'
  | 'timed_out'
  | 'cancelled'
  | 'invalidated_by_config_change'
  | 'invalidated_by_core_restart';
export type ProbeObservationSource = 'manual_outbound' | 'manual_policy' | 'scheduled_policy';

export interface ProbeTargetResult {
  targetTag: string;
  reachable: boolean;
  latencyMs?: number;
  message?: string;
  source: ProbeObservationSource;
  observedAtUnixMs: number;
}

export interface ProbeObservation extends ProbeTargetResult {
  scope: ClientScope;
  jobKind: ProbeJobKind;
  policyTag?: string;
  selectedTag?: string;
}

export interface ProbeJobSnapshot {
  id: number;
  scope: ClientScope;
  kind: ProbeJobKind;
  state: ProbeJobState;
  targetTags: string[];
  results: ProbeTargetResult[];
  completed: number;
  succeeded: number;
  failed: number;
  startedAtUnixMs: number;
  updatedAtUnixMs: number;
  deadlineAtUnixMs: number;
}

export interface StartProbeRequest {
  kind: ProbeJobKind;
  targetTags: string[];
  timeoutMs?: number;
}

export interface StableNodeId {
  profileId: string;
  configRevision: number;
  tag: string;
}

export interface StablePolicyId {
  profileId: string;
  configRevision: number;
  tag: string;
}

export type NodeObservationSource =
  | 'runtime_snapshot'
  | 'manual_outbound'
  | 'manual_policy'
  | 'scheduled_policy';

export interface NodeSnapshot {
  id: StableNodeId;
  tag: string;
  protocol: string;
  server?: string;
  port?: number;
  udp?: boolean;
  network?: string;
  tls?: boolean;
  sni?: string;
  cipher?: string;
  groupTags: string[];
  selectedIn: string[];
  runtimeAvailable: boolean;
  alive?: boolean;
  latencyMs?: number;
  lastObservedAtUnixMs?: number;
  lastObservationSource?: NodeObservationSource;
  activeProbeJobIds: number[];
  history: ProbeObservation[];
  actionValid: boolean;
}

export interface NodeGroupSnapshot {
  id: StablePolicyId;
  tag: string;
  kind: string;
  selected?: string;
  memberTags: string[];
  runtimeAvailable: boolean;
  available: boolean;
  reason?: string;
}

export interface NodeScreenSnapshot {
  revision: number;
  scope: ClientScope;
  sourceStatus: ClientSourceStatus;
  groups: NodeGroupSnapshot[];
  nodes: NodeSnapshot[];
  activeProbeJobs: ProbeJobSnapshot[];
}

export interface ConnectionStatus {
  state: ConnectionState;
  message?: string;
  uptimeMs?: number;
  startedAtUnixMs?: number;
  activeConnections?: number;
  coreAvailable?: boolean;
  systemProxyEnabled?: boolean;
  /** Process details from backend CoreProcessStatus */
  processState?: string;
  processPid?: number | null;
  processExitCode?: number | null;
  processExitReason?: string;
  processEndpointPath?: string;
  localProxyHost?: string;
  localProxyPort?: number;
}

export interface ProxyModeStatus {
  currentMode: ProxyMode;
  availableModes: ProxyMode[];
  message?: string;
}

export interface CoreOverview {
  coreState: 'stopped' | 'starting' | 'running' | 'stopping' | 'error';
  version?: string;
  uptimeMs?: number;
  memoryUsageBytes?: number;
  cpuUsagePercent?: number;
}

export interface TrafficStats {
  uploadBytesPerSec: number;
  downloadBytesPerSec: number;
  totalUploadBytes: number;
  totalDownloadBytes: number;
  connectionCount: number;
}

export interface TrafficRateSample extends TrafficStats {
  sampledAtUnixMs: number;
  stable: boolean;
}

export interface ConfigProxyNode {
  tag: string;
  protocol: string;
  isSelector: boolean;
  /** Server hostname / IP (from `protocol.server`). */
  server?: string;
  /** Remote port (from `protocol.port`). */
  port?: number;
  /** Whether the outbound supports UDP relay. */
  udp?: boolean;
  /** Transport network type: tcp / ws / grpc / h2. */
  network?: string;
  /** Whether TLS is enabled. */
  tls?: boolean;
  /** Server Name Indication. */
  sni?: string;
  /** Cipher / encryption algorithm. */
  cipher?: string;
}

export interface PolicyOutbound {
  tag: string;
  type: string;
  delayMs?: number;
  alive?: boolean;
  lastCheckedUnixMs?: number;
  lastError?: string;
}

export interface PolicyProbeCompletedEvent {
  policyTag: string;
  trigger?: 'startup' | 'scheduled' | 'manual' | string;
  url?: string;
  startedAtUnixMs?: number;
  completedAtUnixMs?: number;
  durationMs?: number;
  selected?: string;
  members: PolicyOutbound[];
}

export interface PolicyProbeAccepted {
  accepted: boolean;
  result?: {
    policyTag?: string;
    probeTriggered?: boolean;
  };
}

export interface PolicyGroup {
  name: string;
  kind?: string;
  selected?: string;
  outbounds: PolicyOutbound[];
}

export interface GuiCoreHealth {
  healthy: boolean;
  engineVersion?: string;
  startedAtUnixMs?: number;
}

export interface GuiZeroCapabilities {
  available: boolean;
  apiVersion?: string;
  schemaVersion?: string;
  features: string[];
  permissions: string[];
  adapters: GuiCapabilityEndpoint[];
  sinks: GuiCapabilityEndpoint[];
  protocols: GuiProtocolCapability[];
  buildFeatures: string[];
  error?: string;
}

export interface GuiCapabilityEndpoint {
  kind: string;
  enabled: boolean;
}

export interface GuiProtocolCapability {
  name: string;
  status: 'supported' | 'partial' | 'experimental';
  inboundTcp: boolean;
  inboundUdp: boolean;
  outboundTcp: boolean;
  outboundUdp: boolean;
  mux: boolean;
  limitations: string[];
}

export interface GuiFeatureStatus {
  key: string;
  supported: boolean;
  enabled: boolean;
  state: string;
  reason?: string;
}

export interface GuiFakeIpClearInput {
  domain?: string;
  ip?: string;
}

export interface GuiFakeIpClearResult {
  coreInstanceId?: string;
  configRevision?: number;
  enabled: boolean;
  scope: 'all' | 'domain' | 'ip';
  domain?: string;
  ip?: string;
  removedMappings: number;
  removedAddresses: number;
  liveMappings: number;
}

export interface GuiTunStatus extends GuiFeatureStatus {
  name?: string;
  addr?: string;
  addresses: string[];
  mtu?: number;
  tag?: string;
  healthy: boolean;
  autoRoute: boolean;
  dualStack: boolean;
  strictRoute: boolean;
  dnsHijack: boolean;
  egressInterface?: string;
  egressInterfaceV4?: string;
  egressInterfaceV6?: string;
  lastError?: string;
  managedByConfig: boolean;
}

export interface GuiPolicySelectionResult {
  policyTag: string;
  targetTag: string;
  selected?: string;
  accepted: boolean;
  message?: string;
}

export interface GuiTargetProbeResult {
  targetTag: string;
  reachable: boolean;
  latencyMs?: number;
  server?: string;
  port?: number;
  message?: string;
}

export interface GuiConnectionNetworkInterface {
  name: string;
  index?: number;
}

export interface GuiConnectionEgressContext {
  generation?: number;
  addressFamily?: string;
  tunActive?: boolean;
  configuredInterface?: GuiConnectionNetworkInterface;
  unavailableReason?: string;
}

export interface GuiConnectionRouteLookup {
  status?: string;
  sourceAddress?: string;
  error?: string;
}

export interface GuiConnectionSocketBinding {
  mode?: string;
  reason?: string;
  interfaceBound?: boolean;
}

export interface GuiConnectionNetworkContext {
  localAddress?: string;
  remoteAddress?: string;
  resolvedCandidates: string[];
  selectedInterface?: GuiConnectionNetworkInterface;
  egress?: GuiConnectionEgressContext;
  routeLookup?: GuiConnectionRouteLookup;
  socketBinding?: GuiConnectionSocketBinding;
  connectStage?: string;
}

export interface GuiConnectionItem {
  flowId: string;
  revision?: number;
  state?: 'opening' | 'active' | 'completed' | string;
  network: string;
  source?: string;
  sourceIp?: string;
  sourcePort?: number;
  processId?: number;
  processName?: string;
  processPath?: string;
  destination: string;
  targetHost?: string;
  targetIp?: string;
  targetPort?: number;
  originalIp?: string;
  hostSource?: string;
  fakeIpReverseStatus?: string;
  sniffedHost?: string;
  inboundTag?: string;
  inboundProtocol?: string;
  outboundTag?: string;
  outboundProtocol?: string;
  remoteDestination?: string;
  networkContext?: GuiConnectionNetworkContext;
  policyTag?: string;
  routeMode?: string;
  routeAction?: string;
  matchedRuleIndex?: number;
  matchedRule?: string;
  selectionChain: string[];
  relayChain: string[];
  outcome?: string;
  closeReason?: string;
  failureStage?: string;
  failureCode?: string;
  failureMessage?: string;
  bytesUp: number;
  bytesDown: number;
  inboundRxBytes?: number;
  inboundTxBytes?: number;
  outboundRxBytes?: number;
  outboundTxBytes?: number;
  throughputUpBps?: number;
  throughputDownBps?: number;
  startedAtUnixMs?: number;
  lastActivityAtUnixMs?: number;
  endedAtUnixMs?: number;
  updatedAtUnixMs?: number;
  durationMs?: number;
}

export interface GuiConnectionList {
  items: GuiConnectionItem[];
  total?: number;
  limit: number;
}

export interface GuiConnectionCloseResult {
  flowId: string;
  closed: boolean;
  message?: string;
}

// ── Config plan-apply impact analysis ──

export interface ConfigImpactItem {
  /** Top-level config section (e.g. "outbounds", "listeners", "rules", "tun"). */
  section: string;
  /** Specific tags/identifiers within the section that changed. */
  tags: string[];
  /** Human-readable description of the change. */
  detail: string;
}

export interface ConfigPlanApplyResult {
  /** Whether the proposed config is syntactically and semantically valid. */
  valid: boolean;
  /** Sections that can be hot-reloaded without restarting the kernel. */
  hotReload: ConfigImpactItem[];
  /** Sections that require a kernel restart to take effect. */
  requiresRestart: ConfigImpactItem[];
  /** Non-blocking warnings about side effects. */
  warnings: string[];
  /** Validation errors (present when `valid` is false). */
  errors: string[];
}

