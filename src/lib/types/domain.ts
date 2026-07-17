// Mirror of Rust models::proxy_config, subscription, rule_set

export interface ProxyConfigProfile {
  id: string;
  name: string;
  kernel: string;
  format: string;
  path?: string;
  content?: unknown;
  active: boolean;
  updatedAtUnixMs: number;
  capabilities: ProxyConfigCapabilities;
}

export interface ProxyConfigUpsert {
  id?: string;
  name: string;
  kernel?: string;
  format?: string;
  path?: string;
  content?: unknown;
  active?: boolean;
}

export interface ProxyConfigImport {
  id?: string;
  name: string;
  kernel?: string;
  format?: string;
  path?: string;
  content?: string;
  active?: boolean;
}

export interface ProxyConfigCapabilities {
  hasProxyNodes: boolean;
  hasProxyGroups: boolean;
  hasRouteRules: boolean;
  hasRuleSets: boolean;
  hasSelector: boolean;
  hasUrlTest: boolean;
  featureKeys: string[];
}

export interface SubscriptionProfile {
  id: string;
  name: string;
  url: string;
  enabled: boolean;
  kernel: string;
  format: string;
  targetProxyConfigId?: string;
  /** Auto-sync interval in seconds. When set and enabled, the
   * background scheduler re-syncs this subscription. */
  updateIntervalSecs?: number;
  /** Custom User-Agent used when fetching this subscription. */
  userAgent?: string;
  /** Number of proxy nodes detected during the last sync. */
  nodeCount?: number;
  uploadBytes?: number;
  downloadBytes?: number;
  totalBytes?: number;
  /** Subscription expiry (ms since epoch). */
  expireAtUnixMs?: number;
  updatedAtUnixMs: number;
  lastSyncAtUnixMs?: number;
  lastError?: string;
}

export interface SubscriptionUpsert {
  id?: string;
  name: string;
  url: string;
  enabled?: boolean;
  kernel?: string;
  format?: string;
  targetProxyConfigId?: string;
  updateIntervalSecs?: number;
  userAgent?: string;
}

export interface SubscriptionSyncAllOutcome {
  total: number;
  succeeded: number;
  failed: number;
}

export interface RuleSetProfile {
  id: string;
  name: string;
  enabled: boolean;
  semanticIr: ZeroRuleIr;
  source?: RuleSetSource;
  sourceState: RuleSetSourceState;
  artifact?: ZrsArtifact;
  updatedAtUnixMs: number;
  lastSyncAtUnixMs?: number;
  lastError?: string;
}

export type ZeroRuleType = 'domain_exact' | 'domain_suffix' | 'domain_keyword' | 'ipv4_cidr' | 'ipv6_cidr';
export interface ZeroRule { type: ZeroRuleType; value: string; }
export interface ZeroRuleIr { version: 1; name?: string; rules: ZeroRule[]; }
export interface RuleSetKernelPayload { id: string; name: string; zrsPath: string; checksum: number; }

export interface RuleSetSource {
  url: string;
  format: 'auto' | 'zero-rule-ir-v1' | 'clash-classical-yaml';
  updateIntervalSecs?: number;
  userAgent?: string;
}

export interface RuleSetSourceState {
  etag?: string;
  lastModified?: string;
  contentSha256?: string;
  contentBytes?: number;
  lastCheckedAtUnixMs?: number;
}

export interface ZrsArtifact {
  path: string;
  majorVersion: number;
  minorVersion: number;
  checksum: number;
  fileSize: number;
  entryCount: number;
  builtAtUnixMs: number;
}

export interface RuleSetUpsert {
  id?: string;
  name: string;
  enabled?: boolean;
  semanticIr?: ZeroRuleIr;
  source?: RuleSetSource;
}

export interface RuleSetSyncAllOutcome {
  total: number;
  updated: number;
  unchanged: number;
  failed: number;
}
