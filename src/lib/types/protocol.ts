// UI-level display types (not mirrors of Rust models)

/**
 * Display-time proxy node — the composite shape consumed by node cards.
 *
 * Static attributes (protocol, server, udp, tls, …) come from the active
 * config file via `ConfigProxyNode`.  Runtime status (selected, delay,
 * alive) is layered on top from the kernel's Policies query when the
 * core is connected.  The Nodes tab merges both sources into this type.
 */
export interface ProxyNode {
  /** Stable unique key (usually the outbound tag). */
  id: string;
  /** Raw outbound tag — sent back to the kernel on `policies.select`. */
  tag: string;
  /** Display name shown to the user (may contain emoji). */
  name: string;
  /** Emoji / flag extracted from the name, if present. */
  emoji?: string;
  /** Cleaned name with leading emoji stripped. */
  cleanName?: string;
  /** Protocol family: shadowsocks / vmess / trojan / … */
  protocol: string;
  /** Latency in ms; `0` means "not tested yet". */
  delay: number;
  /** Unix-ms timestamp of the last probe; `undefined` if never probed. */
  lastProbeAt?: number;
  /** Display domain / grouping hint (e.g. policy group name). */
  domain: string;
  // ── Static attributes (from config) ──
  server?: string;
  port?: number;
  udp?: boolean;
  network?: string;
  tls?: boolean;
  sni?: string;
  cipher?: string;
  // ── Runtime overlay ──
  /** Currently selected outbound in its policy group. */
  selected?: boolean;
  /** Reachability flag from the last probe. */
  alive?: boolean;
}
