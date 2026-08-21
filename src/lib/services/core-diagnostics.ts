import { invoke } from '@tauri-apps/api/core';
import type { CoreCallResult, CoreIpcOptions } from '$lib/types/core';

/**
 * Read-only diagnostics bridge.
 *
 * Zero core exposes diagnostics through the generic query plane. Keep the
 * frontend contract generic until core publishes stable query payload schemas.
 */
export async function queryCoreDiagnostics(
  method: string,
  payload?: unknown,
  options?: CoreIpcOptions,
): Promise<CoreCallResult> {
  return invoke('core_ipc_query', {
    request: {
      method,
      payload: payload ?? {},
    },
    options,
  });
}

export async function getTunRuntimeDiagnostics(
  options?: CoreIpcOptions,
): Promise<CoreCallResult> {
  return queryCoreDiagnostics('tun.runtime', undefined, options);
}

export async function getDnsDiagnostics(
  options?: CoreIpcOptions,
): Promise<CoreCallResult> {
  return queryCoreDiagnostics('dns.runtime', undefined, options);
}
