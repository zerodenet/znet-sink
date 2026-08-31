export type DnsMode = 'disabled' | 'real' | 'fake_ip';
export type DnsServerType = 'system' | 'udp' | 'doh' | 'dot' | 'doq';
export type DnsAddressFamilyPolicy = 'ipv4_only' | 'ipv6_only' | 'prefer_ipv4' | 'prefer_ipv6';

export interface DnsServerConfig {
  type: DnsServerType;
  host?: string;
  port?: number;
  path?: string;
  bootstrap?: string[];
  server_name?: string;
  [key: string]: unknown;
}

export interface DnsDispatchConfig {
  condition: Record<string, unknown>;
  server: string;
  [key: string]: unknown;
}

export interface DnsCacheConfig {
  max_entries: number;
  max_ttl_seconds?: number;
  [key: string]: unknown;
}

export interface DnsPolicyConfig {
  address_family?: DnsAddressFamilyPolicy;
  [key: string]: unknown;
}

export type DnsAnswerConfig =
  | { type: 'real'; [key: string]: unknown }
  | {
      type: 'fake_ip';
      cidr: string;
      ipv6_cidr?: string;
      ttl_seconds: number;
      max_entries?: number;
      exclude_domains: string[];
      [key: string]: unknown;
    };

export interface DnsConfig {
  servers: Record<string, DnsServerConfig>;
  default_server: string;
  dispatch: DnsDispatchConfig[];
  cache?: DnsCacheConfig;
  answer: DnsAnswerConfig;
  policy?: DnsPolicyConfig;
  [key: string]: unknown;
}

export interface DnsSettingsDraft {
  mode: DnsMode;
  dns: DnsConfig;
  dnsHijack: boolean;
  advanced: boolean;
}

export interface DnsSettingsInput {
  enabled: boolean;
  config?: DnsConfig;
  dnsHijack: boolean;
}

export interface DnsDraftIssue {
  field: string;
  message: string;
  severity: 'error' | 'warning';
}
