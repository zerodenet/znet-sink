import { invoke } from '@tauri-apps/api/core';
import type { DnsLookupResult, DnsCacheResult, FakeIpLookupResult } from '$lib/types/diagnostics';
import type { GuiFakeIpClearInput, GuiFakeIpClearResult } from '$lib/types/gui-api';
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
