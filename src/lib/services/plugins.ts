import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { proxyConfigSignal } from './proxy-config-signal.svelte';
import { ruleSetSignal } from './rule-set-signal.svelte';
export interface PluginPermission { capability: string; scope: string }
export type PluginSdkMethod =
  | 'storage_get' | 'storage_put' | 'storage_delete' | 'storage_list' | 'storage_export' | 'storage_clear' | 'storage_migrate'
  | 'notification_post' | 'schedule_put' | 'schedule_list' | 'schedule_delete'
  | 'browser_open' | 'callback_create' | 'callback_poll' | 'callback_cancel'
  | 'file_read' | 'file_write' | 'material_submit' | 'material_drop'
  | 'secret_receive' | 'configured_request' | 'crypto_use'
  | 'persistent_secret_get' | 'persistent_secret_put' | 'persistent_secret_delete'
  | 'crypto_key_generate' | 'crypto_sign' | 'crypto_verify' | 'crypto_digest'
  | 'crypto_hpke_key_generate' | 'crypto_hpke_seal' | 'crypto_hpke_open'
  | 'subscription_apply' | 'subscription_metadata_update' | 'subscription_sync_complete' | 'subscription_remove' | 'protected_load';
export interface PluginSdkBudget { timeout_ms?: number; max_result_bytes?: number }
export interface PluginSdkCall { version: 1; request: PluginPermission; method: PluginSdkMethod; budget?: PluginSdkBudget; arguments?: unknown }
export interface PluginSdkReply<T = unknown> {
  version: 1; ok: boolean; value?: T;
  error?: { code: string; message: string; field_path?: string; diagnostics?: string[]; retry_after_ms?: number };
}
export interface PluginReview { key: string; identity: string; registration: number; revision: number; host_generation?: string }
export interface PluginPlatform {
  state: 'starting' | 'ready' | 'suspended' | 'unavailable' | 'restricted';
  target: {os: 'android' | 'ios'; arch: string; device: 'phone' | 'tablet'} | null;
  generation: string; executionAllowed: boolean; externalInstallationAllowed: boolean;
  isolation: 'bounded-in-process-interpreter'; reason: string | null;
  secureStorageAdapter?: boolean;
  taskScheduling?: { mode: 'foreground-only'; state: 'available' | 'deferred'; reason: string };
}
export interface PluginConfigurationOption { value: string; label: string }
export interface PluginConfigurationField {
  id: string; label: string; description?: string; kind: 'text' | 'https_origin' | 'https_origin_list' | 'select';
  required: boolean; default?: string; options: PluginConfigurationOption[];
}
export interface PluginConfiguration {
  schema: { title: string; description?: string; fields: PluginConfigurationField[] };
  values: Record<string, string>;
  configured: boolean;
}
export interface PluginComponent {
  plugin_id: string; name: string; description?: string; component_id: string; version: string; publisher: string;
  repository?: string; homepage?: string | null; documentation?: string | null; license?: string; surfaces?: string[];
  review: PluginReview | null; enabled: boolean; permission_review_required?: boolean; running: boolean; blocked: string | null;
  permissions: Array<{ request: PluginPermission; required: boolean; supported: boolean; granted: boolean; unsupported_reason?: string }>;
  configuration: PluginConfiguration | null;
}
export interface PluginPage { plugin_id: string; id: string; title: string; kind: 'management' }
export interface PluginSnapshot { checked: boolean; components: PluginComponent[]; pages?: PluginPage[]; notices: string[]; platform?: PluginPlatform }
export interface PluginListing {
  product_id?: string; id: string; name: string; description: string; publisher: { id: string }; repository: string;
  surfaces?: string[]; capabilities?: string[];
}
export interface PluginRelease { channel?: 'stable' | 'rc' | 'dev' | null; html_url?: string; tag_name: string; name: string | null; body: string | null; prerelease: boolean; published_at: string | null }
export interface PluginDownloadProgress {
  pluginId: string; tag: string; bytesDownloaded: number; bytesTotal?: number;
  percent?: number; state: 'downloading' | 'retrying' | 'verifying'; attempt: number;
}
export interface PluginPermissionChange { component_id: string; request: PluginPermission; required: boolean }
export interface PluginInstallReview {
  host_generation?: string;
  plugin_id: string; current_version: string | null; candidate_version: string; candidate_digest: string;
  publisher: string; publisher_fingerprint: string; first_install: boolean; local_trust: boolean;
  requested_surfaces: string[]; requires_approval: boolean;
  added_permissions: PluginPermissionChange[]; removed_permissions: PluginPermissionChange[];
}
export const onPluginDownloadProgress = (callback: (progress: PluginDownloadProgress) => void): Promise<UnlistenFn> =>
  listen<PluginDownloadProgress>('plugin:download-progress', event => callback(event.payload));
export type PluginPageDocument = import('$lib/features/plugin-pages/channel').PageDocument;
export const pluginApi = {
  catalog: () => invoke<PluginListing[]>('plugins_catalog'),
  releases: (id: string) => invoke<PluginRelease[]>('plugins_releases', { id }),
  previewRelease: (id: string, tag: string) => invoke<PluginInstallReview>('plugins_preview_release', { id, tag }),
  installRelease: (id: string, tag: string, approvalDigest?: string, hostGeneration?: string) => invoke<PluginSnapshot>('plugins_install_release', { id, tag, approvalDigest, hostGeneration }),
  supported: () => invoke<boolean>('plugins_supported'),
  snapshot: () => invoke<PluginSnapshot>('plugins_snapshot'),
  recheckHost: () => invoke<PluginSnapshot>('plugins_host_recheck'),
  page: (pluginId: string, pageId: string) => invoke<string>('plugins_page', { pluginId, pageId }),
  pageDocument: (pluginId:string,pageId:string)=>invoke<PluginPageDocument>('plugins_page_document',{pluginId,pageId}),
  pageClose: (documentReview:string)=>invoke<void>('plugins_page_close',{documentReview}),
  pageCheck: (documentReview:string)=>invoke<void>('plugins_page_check',{documentReview}),
  pageConfigure: (documentReview:string,review:PluginReview,values:Record<string,string>)=>invoke<PluginSnapshot>('plugins_page_configure',{documentReview,review,values}),
  pageInvoke: (documentReview:string,review:PluginReview,action:string,payload:unknown)=>invoke<unknown>('plugins_page_invoke',{documentReview,review,action,payload}),
  pageSdk: <T>(documentReview:string,review:PluginReview,call:PluginSdkCall)=>invoke<PluginSdkReply<T>>('plugins_page_sdk_call',{documentReview,review,call}),
  pageProtectedLoad: <T>(documentReview:string,review:PluginReview,call:PluginSdkCall)=>invoke<PluginSdkReply<T>>('plugins_page_protected_load',{documentReview,review,call}),
  pagePreflight: (documentReview:string,review:PluginReview,call:PluginSdkCall)=>invoke<PluginSdkReply<{state:string}>>('plugins_page_file_preflight',{documentReview,review,call}),
  refresh: () => invoke<PluginSnapshot>('plugins_refresh'),
  previewInstall: (path: string) => invoke<PluginInstallReview>('plugins_preview_install', { path }),
  pickNativePackage: () => invoke<{ plan_id: string; review: PluginInstallReview } | null>('plugins_native_package_pick'),
  inspectNativePackage: (review: PluginInstallReview) => invoke<{pluginId:string;state:'matching'|'different'|'not-installed';installedVersion:string|null;packageDigest:string|null;observedAtUnixMs:string}>('plugins_native_package_inspect', { target: { pluginId:review.plugin_id,candidateDigest:review.candidate_digest,candidateVersion:review.candidate_version,publisherFingerprint:review.publisher_fingerprint } }),
  confirmNativePackage: (planId: string, approvalDigest: string) => invoke<PluginSnapshot>('plugins_native_package_confirm', { planId, approvalDigest }),
  cancelNativePackage: (planId: string) => invoke<void>('plugins_native_package_cancel', { planId }),
  install: (path: string, approvalDigest?: string) => invoke<PluginSnapshot>('plugins_install', { path, approvalDigest }),
  authorize: (review: PluginReview, grants: PluginPermission[]) => invoke<PluginSnapshot>('plugins_authorize', { review, grants }),
  configure: (key: string, values: Record<string, string>, review?: PluginReview) => invoke<PluginSnapshot>('plugins_configure', { key, values, review }),
  stop: (key: string, review?: PluginReview) => invoke<PluginSnapshot>('plugins_stop', { key, review }),
  revokePermissions: (key: string, review?: PluginReview) => invoke<PluginSnapshot>('plugins_revoke_permissions', { key, review }),
  storage: {
    get: (pluginId: string, area: 'state' | 'cache', key: string) => invoke<string | null>('plugins_storage_get', { pluginId, area, key }),
    put: (pluginId: string, area: 'state' | 'cache', key: string, value: string) => invoke<void>('plugins_storage_put', { pluginId, area, key, value }),
    delete: (pluginId: string, area: 'state' | 'cache', key: string) => invoke<void>('plugins_storage_delete', { pluginId, area, key }),
  },
  run: (review: PluginReview) => invoke<unknown>('plugins_run', { review }),
  invoke: (pluginId: string, componentId: string, action: string, payload: unknown) => invoke<unknown>('plugins_invoke', { pluginId, componentId, action, payload }),
  sdk: <T>(pluginId: string, componentId: string, call: PluginSdkCall) => invoke<PluginSdkReply<T>>('plugins_sdk_call', { pluginId, componentId, call }),
  protectedLoad: <T>(pluginId: string, componentId: string, call: PluginSdkCall) => invoke<PluginSdkReply<T>>('plugins_protected_load', { pluginId, componentId, call }),
  // Legacy calls remain subject to the host's mandatory fixed preview gate.
  uninstall: (id: string) => invoke<PluginSnapshot>('plugins_uninstall', { id }),
};

export type { PluginUninstallPreview, PluginUninstallStatus, PluginUninstallResource } from '$lib/features/plugin-uninstall/types';
export const pluginUninstallApi: import('$lib/features/plugin-uninstall/types').PluginUninstallPorts = {
  preview: pluginId => invoke('plugins_uninstall_preview', { pluginId }),
  execute: input => invoke('plugins_uninstall_execute', input),
  status: operationId => invoke('plugins_uninstall_status', { operationId }),
  discard: planId => invoke('plugins_uninstall_discard', { planId }),
  recent: () => invoke('plugins_uninstall_recent'),
};
/** Refresh observations after partial/unknown cleanup; no runtime reconciliation or writes. */
export async function readPluginUninstallObservations(): Promise<PluginSnapshot | null> {
  proxyConfigSignal.markChanged(true);
  ruleSetSignal.markChanged();
  const [snapshot] = await Promise.allSettled([
    pluginApi.snapshot(), invoke('gui_tun_status'),
  ]);
  return snapshot.status === 'fulfilled' ? snapshot.value : null;
}
