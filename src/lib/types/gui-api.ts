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
  checks: SelfTestCheckItem[];
  suggestedFlow: 'setup' | 'ready' | 'troubleshoot';
}

export type ConnectionState = 'disconnected' | 'connecting' | 'connected' | 'error';
export type ProxyMode = 'global' | 'rule' | 'direct';

export interface ConnectionStatus {
  state: ConnectionState;
  message?: string;
  uptimeMs?: number;
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

export interface ConfigProxyNode {
  tag: string;
  protocol: string;
  isSelector: boolean;
}

export interface PolicyOutbound {
  tag: string;
  type: string;
  delayMs?: number;
  alive?: boolean;
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
  error?: string;
}

export interface GuiCapabilityEndpoint {
  kind: string;
  enabled: boolean;
}

export interface GuiFeatureStatus {
  key: string;
  supported: boolean;
  enabled: boolean;
  state: string;
  reason?: string;
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

export interface GuiConnectionItem {
  flowId: string;
  network: string;
  source?: string;
  destination: string;
  inboundTag?: string;
  outboundTag?: string;
  policyTag?: string;
  routeMode?: string;
  outcome?: string;
  bytesUp: number;
  bytesDown: number;
  throughputUpBps?: number;
  throughputDownBps?: number;
  startedAtUnixMs?: number;
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
