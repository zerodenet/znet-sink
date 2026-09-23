/** ZNet Sink plugin SDK v1. This module contains no Tauri or DOM privileges. */
export const SDK_VERSION = 1 as const;

export type Capability =
  | 'plugin.self.read' | 'records.summary.read'
  | 'network.get' | 'network.request' | 'network.configured.request'
  | 'plugin.storage.read' | 'plugin.storage.write'
  | 'notifications.post' | 'tasks.schedule'
  | 'browser.open' | 'browser.callback'
  | 'files.selection.read' | 'files.selection.write'
  | 'materials.submit' | 'secrets.session.receive'
  | 'crypto.session.use' | 'crypto.device.use'
  | 'secrets.persistent.read' | 'secrets.persistent.write'
  | 'subscriptions.manage' | 'runtime.protected.load';

export interface Permission { capability: Capability; scope: string }
export type Method =
  | 'storage_get' | 'storage_put' | 'storage_delete' | 'storage_list' | 'storage_export' | 'storage_clear' | 'storage_migrate'
  | 'notification_post' | 'schedule_put' | 'schedule_list' | 'schedule_delete'
  | 'browser_open' | 'callback_create' | 'callback_poll' | 'callback_cancel'
  | 'file_read' | 'file_write' | 'material_submit' | 'material_drop'
  | 'secret_receive' | 'configured_request' | 'crypto_use'
  | 'persistent_secret_get' | 'persistent_secret_put' | 'persistent_secret_delete'
  | 'crypto_key_generate' | 'crypto_sign' | 'crypto_verify' | 'crypto_digest' | 'crypto_hpke_key_generate' | 'crypto_hpke_seal' | 'crypto_hpke_open'
  | 'subscription_apply' | 'subscription_metadata_update' | 'subscription_remove' | 'protected_load';

export interface Call {
  version: typeof SDK_VERSION;
  request: Permission;
  method: Method;
  budget?: { timeout_ms?: number; max_result_bytes?: number };
  arguments?: unknown;
}
export interface Budget { timeout_ms?: number; max_result_bytes?: number }
export interface ManagedSubscriptionUsage {
  usedBytes: number;
  totalBytes: number;
  expireAtUnixMs: number;
}
export type ErrorCode =
  | 'invalid_request' | 'unsupported' | 'permission_denied' | 'disabled'
  | 'revoked' | 'busy' | 'expired' | 'cancelled' | 'deadline'
  | 'budget_exceeded' | 'not_found' | 'transport' | 'uncertain';
export interface Failure {
  code: ErrorCode;
  message: string;
  field_path?: string;
  diagnostics?: string[];
  retry_after_ms?: number;
}
export interface Reply<T = unknown> { version: typeof SDK_VERSION; ok: boolean; value?: T; error?: Failure }
export type Transport = <T = unknown>(call: Call) => Promise<Reply<T>>;

export function createSdk(transport: Transport) {
  const call = async <T>(request: Permission, method: Method, argumentsValue: unknown = {}, budget?: Budget): Promise<T> => {
    const reply = await transport<T>({ version: SDK_VERSION, request, method, arguments: argumentsValue, ...(budget ? { budget } : {}) });
    if (!reply.ok) {
      const error = new Error(reply.error?.message || 'Plugin host operation failed');
      Object.assign(error, {
        code: reply.error?.code,
        fieldPath: reply.error?.field_path,
        diagnostics: reply.error?.diagnostics ?? [],
        retryAfterMs: reply.error?.retry_after_ms,
      });
      throw error;
    }
    return reply.value as T;
  };
  return Object.freeze({
    version: SDK_VERSION,
    call,
    storage: Object.freeze({
      get: (area: 'state' | 'cache', key: string) => call<string | null>({ capability: 'plugin.storage.read', scope: 'self' }, 'storage_get', { area, key }),
      put: (area: 'state' | 'cache', key: string, value: string) => call<boolean>({ capability: 'plugin.storage.write', scope: 'self' }, 'storage_put', { area, key, value }),
      delete: (area: 'state' | 'cache', key: string) => call<boolean>({ capability: 'plugin.storage.write', scope: 'self' }, 'storage_delete', { area, key }),
      list: (area: 'state' | 'cache') => call<{ keys: string[] }>({ capability: 'plugin.storage.read', scope: 'self' }, 'storage_list', { area }),
      export: (area: 'state' | 'cache') => call<{ schemaVersion: number; values: Record<string, string> }>({ capability: 'plugin.storage.read', scope: 'self' }, 'storage_export', { area }),
      clear: (area: 'state' | 'cache') => call<boolean>({ capability: 'plugin.storage.write', scope: 'self' }, 'storage_clear', { area }),
      migrate: (area: 'state' | 'cache', from: number, to: number, values: Record<string, string>) => call<boolean>({ capability: 'plugin.storage.write', scope: 'self' }, 'storage_migrate', { area, from, to, values }),
    }),
    notifications: Object.freeze({
      post: (message: string, options: {
        kind?: 'info' | 'success' | 'warning' | 'error';
        duration_ms?: number;
        action?: { pageId: string; route: string; reference?: string };
      } = {}) =>
        call({ capability: 'notifications.post', scope: 'self' }, 'notification_post', { message, ...options }),
    }),
    tasks: Object.freeze({
      put: (taskId: string, action: string, intervalSeconds: number) => call({ capability: 'tasks.schedule', scope: 'self' }, 'schedule_put', { taskId, action, intervalSeconds }),
      list: () => call({ capability: 'tasks.schedule', scope: 'self' }, 'schedule_list'),
      delete: (taskId: string) => call({ capability: 'tasks.schedule', scope: 'self' }, 'schedule_delete', { taskId }),
    }),
    subscriptions: Object.freeze({
      updateMetadata: (providerId: string, remoteSubscriptionId: string, usage: ManagedSubscriptionUsage) =>
        call({ capability: 'subscriptions.manage', scope: 'self' }, 'subscription_metadata_update', {
          providerId,
          remoteSubscriptionId,
          usage,
        }),
    }),
    browser: Object.freeze({
      open: (origin: string, url: string) => call({ capability: 'browser.open', scope: origin }, 'browser_open', { url }),
      createCallback: () => call({ capability: 'browser.callback', scope: 'self' }, 'callback_create'),
      pollCallback: (sessionId: string) => call({ capability: 'browser.callback', scope: 'self' }, 'callback_poll', { sessionId }),
      cancelCallback: (sessionId: string) => call({ capability: 'browser.callback', scope: 'self' }, 'callback_cancel', { sessionId }),
    }),
    files: Object.freeze({
      read: (options: { maxBytes?: number } = {}) => call({ capability: 'files.selection.read', scope: 'user' }, 'file_read', options),
      write: (dataBase64: string) => call({ capability: 'files.selection.write', scope: 'user' }, 'file_write', { dataBase64 }),
    }),
    materials: Object.freeze({
      submit: (dataBase64: string, options: { purpose?: string; lifetimeMs?: number } = {}) => call({ capability: 'materials.submit', scope: 'configuration' }, 'material_submit', { dataBase64, ...options }),
      drop: (handle: string) => call({ capability: 'materials.submit', scope: 'configuration' }, 'material_drop', { handle }),
      receiveSecret: (origin: string, options: unknown) => call({ capability: 'secrets.session.receive', scope: origin }, 'secret_receive', options),
      crypto: (argumentsValue: unknown) => call({ capability: 'crypto.session.use', scope: 'self' }, 'crypto_use', argumentsValue),
      loadRuntime: (handle: string) => call({ capability: 'runtime.protected.load', scope: 'active-runtime' }, 'protected_load', { handle }),
    }),
  });
}
