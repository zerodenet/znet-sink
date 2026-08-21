import type { CoreCallResult, CoreIpcOptions } from '$lib/types/core';
import { guiDnsCache, guiFakeIpLookup } from '$lib/services/core';
import type { DnsCacheResult, FakeIpLookupResult } from '$lib/types/diagnostics';

/**
 * Keep the generic read-only bridge for stable query-plane diagnostics. DNS
 * cache and Fake-IP are command-plane diagnostics in the current Zero API and
 * therefore use their typed adapters below.
 */
export async function queryCoreDiagnostics(
  method: string,
  payload?: unknown,
  options?: CoreIpcOptions,
): Promise<CoreCallResult> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke('core_ipc_query', {
    request: { [method]: payload ?? {} },
    options,
  });
}

export async function getTunRuntimeDiagnostics(
  options?: CoreIpcOptions,
): Promise<CoreCallResult> {
  return queryCoreDiagnostics('tun_status', undefined, options);
}

export async function getDnsDiagnostics(domain?: string): Promise<DnsCacheResult> {
  return guiDnsCache(domain, 256);
}

export async function getFakeIpDiagnostics(
  input: { domain?: string; ip?: string },
): Promise<FakeIpLookupResult> {
  return guiFakeIpLookup(input);
}
