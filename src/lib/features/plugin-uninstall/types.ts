/** Host metadata only: no source bodies, paths, credentials, or guest authority. */
export interface PluginUninstallResource {
  resourceId: string;
  kind: string;
  name: string;
  ownership: 'verified-package' | 'exact-managed-source' | 'verified-component' | 'publisher-namespace';
  dependencies: string[];
  protectedReason: string | null;
}
export interface PluginUninstallPreview {
  planId: string;
  pluginId: string;
  pluginName: string;
  version: string;
  packageDigest: string;
  publisherId: string;
  publisherFingerprint: string;
  hostSessionId: string;
  binding: string;
  expiresAtUnixMs: string;
  canExecute: boolean;
  blockedReasons: string[];
  activeProfile: { id: string; name: string } | null;
  replacementCandidates: { id: string; name: string }[];
  requiresManualSwitch: boolean;
  requiresStop: false;
  resources: PluginUninstallResource[];
  notice: string;
}
export type PluginUninstallItemState = 'pending' | 'executing' | 'completed' | 'residual' | 'unknown' | 'manual';
export type PluginUninstallOperationState = 'prepared' | 'executing' | 'completed' | 'residual' | 'unknown' | 'manual';
export interface PluginUninstallItem {
  resourceId: string;
  kind: string;
  name: string;
  state: PluginUninstallItemState;
  reason: string | null;
}
export interface PluginUninstallStatus {
  operationId: string;
  planId: string;
  pluginId: string;
  pluginName: string;
  version: string;
  packageDigest: string;
  publisherFingerprint: string;
  state: PluginUninstallOperationState;
  reason: string | null;
  startedAtUnixMs: string | null;
  finishedAtUnixMs: string | null;
  items: PluginUninstallItem[];
  canRetry: false;
  notice: string;
}
export interface PluginUninstallPorts {
  preview(pluginId: string): Promise<PluginUninstallPreview>;
  execute(input: { planId: string; operationId: string; confirmOwnedResources: true }): Promise<PluginUninstallStatus>;
  status(operationId: string): Promise<PluginUninstallStatus>;
  discard(planId: string): Promise<void>;
  recent(): Promise<PluginUninstallStatus[]>;
}
export interface PluginUninstallView {
  phase: 'idle' | 'loading' | 'review' | 'submitting' | 'checking' | 'result' | 'records' | 'error';
  preview: PluginUninstallPreview | null;
  status: PluginUninstallStatus | null;
  operationId: string | null;
  acknowledged: boolean;
  message: string | null;
  records: PluginUninstallStatus[];
  recordsError: string | null;
}
