/**
 * Read-only FakeIP diagnostics model.
 *
 * This intentionally mirrors the kernel diagnostic surface only. Editing
 * allocator pools or lifecycle state remains owned by Zero core.
 */
export interface GuiFakeIpEntry {
  domain: string;
  address: string;
  ttlSeconds?: number;
  createdAtUnixMs?: number;
}

export interface GuiFakeIpLookupResult {
  query: string;
  matched: boolean;
  entry?: GuiFakeIpEntry;
}

export interface GuiDnsCacheEntry {
  domain: string;
  addresses: string[];
  ttlSeconds?: number;
  resolver?: string;
}

export interface GuiDnsCacheSnapshot {
  entries: GuiDnsCacheEntry[];
  updatedAtUnixMs?: number;
}
