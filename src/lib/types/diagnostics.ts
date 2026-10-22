// Defensive types for the `diagnostics.dns_lookup` and `diagnostics.trace_route`
// kernel commands. The Zero adapter returns the kernel response without
// normalization, so these shapes cover the common field names while an index
// signature preserves anything else — the UI falls back to a raw JSON view
// when the actual shape diverges.

/** A single DNS resolution record. Field names are permissive because the
 *  kernel may emit any of several naming conventions. */
export interface DnsRecord {
  type?: string;
  name?: string;
  value?: string;
  data?: string;
  ttl?: number;
  preference?: number;
  [key: string]: unknown;
}

/** Result of a `diagnostics.dns_lookup` command. */
export interface DnsLookupResult {
  hostname?: string;
  resolved_addresses?: string[];
  count?: number;
  answers?: DnsRecord[];
  records?: DnsRecord[];
  results?: DnsRecord[];
  rcode?: string | number;
  status?: string;
  server?: string;
  resolver?: string;
  elapsedMs?: number;
  query_role?: string;
  attempts?: DnsBackendAttempt[];
  error?: string;
  [key: string]: unknown;
}

export interface DnsBackendAttempt {
  role?: string;
  server_tag?: string;
  transport?: string;
  server_endpoints?: string[];
  outbound?: string;
  success?: boolean;
  failure_reason?: string;
  [key: string]: unknown;
}

export interface DnsCacheEntry {
  domain: string;
  addresses: string[];
  ttl_seconds?: string | null;
  [key: string]: unknown;
}

export interface DnsCacheResult {
  enabled: boolean;
  domain?: string;
  hit?: boolean;
  addresses?: string[];
  ttl_seconds?: string | null;
  entries?: DnsCacheEntry[];
  count?: number;
  [key: string]: unknown;
}

export interface FakeIpStats {
  allocations: number;
  expirations: number;
  evictions: number;
  exhaustions: number;
  collisions: number;
  reverse_misses: number;
  live_mappings: number;
  retired_addresses: number;
  capacity: number;
  [key: string]: unknown;
}

export interface FakeIpLookupResult {
  enabled: boolean;
  domain?: string;
  ip?: string;
  fake_ip?: string;
  stats?: FakeIpStats;
  [key: string]: unknown;
}

/** Actual public route evaluation, not a traceroute or reachability probe.
 * Rule indices are decimal strings to preserve the native usize/u64 wire. */
export interface TraceRouteResult {
  target: string;
  port: number;
  protocol: 'tcp' | 'udp';
  inbound_tag: string | null;
  effective_mode: 'rule' | 'global' | 'direct';
  route_action: 'direct' | 'reject' | { route: string };
  matched_rule: { index: string; condition: string } | null;
}
