// Mirror of Rust models::core, models::core_process, models::core_config, events::emitter

export type CoreProcessState = 'notStarted' | 'starting' | 'running' | 'exited' | 'failed';

export type CoreProcessExitReason = 'stopped' | 'exited' | 'crashed';

export interface CoreProcessStatus {
  state: CoreProcessState;
  pid?: number;
  kernel: string;
  executablePath?: string;
  workingDir?: string;
  configPath?: string;
  endpointPath: string;
  startedAtUnixMs?: number;
  exitedAtUnixMs?: number;
  exitCode?: number;
  exitReason?: CoreProcessExitReason;
  lastError?: string;
}

export interface CoreEndpoint {
  transport: string;   // "unix-socket" | "named-pipe"
  path: string;
}

export interface CoreIpcOptions {
  socket?: string;
  timeoutMs?: number;
}

export interface CoreCallResult {
  available: boolean;
  endpoint: CoreEndpoint;
  response?: unknown;
  error?: AppError;
}

export interface CoreEventSubscription {
  generation: number;
  eventName: string;
  statusEventName: string;
}

export interface CoreEventPayload {
  generation: number;
  event: unknown;
}

export type GuiEventType =
  | 'core.statusChanged'
  | 'core.warning'
  | 'core.configChanged'
  | 'connection.started'
  | 'connection.updated'
  | 'connection.closed'
  | 'policy.selected'
  | 'policy.probeCompleted'
  | 'traffic.sampled'
  | 'core.unknownEvent';

export interface GuiEventEnvelope {
  eventType: GuiEventType;
  sourceEventType: string;
  eventId?: string;
  sequence?: number;
  occurredAtUnixMs?: number;
  payload?: {
    kind: string;
    data?: unknown;
  };
}

export interface GuiEventPayload {
  generation: number;
  event: GuiEventEnvelope;
}

export type CoreEventStatusKind = 'subscribed' | 'disconnected' | 'stopped' | 'offline' | 'error';

export interface CoreEventStatus {
  generation: number;
  status: CoreEventStatusKind;
  error?: AppError;
  response?: unknown;
}

export interface CoreConfigSnapshot {
  kernel: string;
  autoConnect: boolean;
  autoStart: boolean;
  executablePath?: string;
  executableExists: boolean;
  configPath?: string;
  configExists?: boolean;
  workingDir?: string;
  workingDirExists?: boolean;
  socket?: string;
  endpoint: CoreEndpoint;
  launchArgs: string[];
  warnings: string[];
}

export interface CoreKernelInfo {
  kernel: string;
  executablePath?: string;
  executableExists: boolean;
  fileName?: string;
  sizeBytes?: number;
  modifiedAtUnixMs?: number;
  recommendedInstallDir?: string;
  downloadUrl?: string;
  warnings: string[];
}

export interface CoreConfigExportResult {
  proxyConfigId: string;
  path: string;
  appConfig: CoreConfigSnapshot;
}

export interface AppError {
  code: string;
  message: string;
  details?: unknown;
}
