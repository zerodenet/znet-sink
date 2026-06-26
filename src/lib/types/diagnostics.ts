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
  answers?: DnsRecord[];
  records?: DnsRecord[];
  results?: DnsRecord[];
  rcode?: string | number;
  status?: string;
  server?: string;
  resolver?: string;
  elapsedMs?: number;
  error?: string;
  [key: string]: unknown;
}

/** A single hop in a route trace. `rtt` may be a single value or an array
 *  of probe attempts depending on the kernel response. */
export interface TraceHop {
  ttl?: number;
  hop?: number;
  addr?: string;
  address?: string;
  ip?: string;
  host?: string;
  rtt?: number | number[];
  latency?: number | number[];
  timeout?: boolean;
  error?: string;
  [key: string]: unknown;
}

/** Result of a `diagnostics.trace_route` command. */
export interface TraceRouteResult {
  target?: string;
  port?: number;
  protocol?: string;
  hops?: TraceHop[];
  totalHops?: number;
  elapsedMs?: number;
  error?: string;
  [key: string]: unknown;
}
