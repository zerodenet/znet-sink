// Mirror of Rust models::app_config

import type { DnsConfig } from './dns';

export interface AppConfig {
  schemaVersion: string;
  core: AppCoreConfig;
  logs: AppLogConfig;
  ui: AppUiConfig;
  localProxy: AppLocalProxyConfig;
  tun: AppTunConfig;
  dns: AppDnsConfig;
  routing: AppRoutingConfig;
  urlTest: AppUrlTestConfig;
}

export interface AppCoreConfig {
  kernel: string;
  autoConnect: boolean;
  autoStart: boolean;
  cleanupProxyOnExit: boolean;
  executablePath?: string;
  downloadUrl?: string;
  configPath?: string;
  workingDir?: string;
  socket?: string;
  networkProbeUrls: string[];
}

export interface AppLogConfig {
  level: string;
  maxEntries: number;
}

export interface AppUiConfig {
  theme: string;        // "light" | "dark" | "system"
  uiMode: string;       // "lite" | "pro"
  sidebarCollapsed: boolean;
  hiddenMenuKeys: string[];
  trafficBallEnabled: boolean;
  defaultRoute?: string;
}

export interface AppLocalProxyConfig {
  host: string;
  port: number;
  sourceProxyConfigId?: string;
  bypass: string[];
}

export interface AppTunConfig {
  /** Explicit local desired state. Undefined preserves legacy Lite auto-connect until first toggle. */
  enabled?: boolean;
  name?: string;
  addr: string;
  mask: string;
  secondaryAddr?: string;
  tag: string;
  mtu: number;
  dualStack: boolean;
  dnsHijack: boolean;
}

export interface AppDnsConfig {
  enabled: boolean;
  config?: DnsConfig;
  dnsHijack: boolean;
}

export interface AppRoutingConfig {
  injectCommonRules: boolean;
}

export interface AppUrlTestConfig {
  toleranceMs: number;
}

export interface KernelSettingsExportResult {
  path: string;
  schemaVersion: string;
}

// Patch types for partial updates

export interface AppConfigPatch {
  core?: AppCoreConfigPatch;
  logs?: AppLogConfigPatch;
  ui?: AppUiConfigPatch;
  localProxy?: AppLocalProxyConfigPatch;
  tun?: AppTunConfigPatch;
  dns?: AppDnsConfigPatch;
  routing?: AppRoutingConfigPatch;
  urlTest?: AppUrlTestConfigPatch;
}

export interface AppCoreConfigPatch {
  kernel?: string;
  autoConnect?: boolean;
  autoStart?: boolean;
  cleanupProxyOnExit?: boolean;
  executablePath?: string | null;
  downloadUrl?: string | null;
  configPath?: string | null;
  workingDir?: string | null;
  socket?: string | null;
  networkProbeUrls?: string[];
}

export interface AppLogConfigPatch {
  level?: string;
  maxEntries?: number;
}

export interface AppUiConfigPatch {
  theme?: string;
  uiMode?: string;
  sidebarCollapsed?: boolean;
  hiddenMenuKeys?: string[];
  trafficBallEnabled?: boolean;
  defaultRoute?: string | null;
}

export interface AppLocalProxyConfigPatch {
  host?: string;
  port?: number;
  sourceProxyConfigId?: string | null;
  bypass?: string[];
}

export interface AppTunConfigPatch {
  enabled?: boolean;
  name?: string | null;
  addr?: string;
  mask?: string;
  secondaryAddr?: string | null;
  tag?: string;
  mtu?: number;
  dualStack?: boolean;
  dnsHijack?: boolean;
}

export interface AppDnsConfigPatch {
  enabled?: boolean;
  config?: DnsConfig | null;
  dnsHijack?: boolean;
}

export interface AppRoutingConfigPatch {
  injectCommonRules?: boolean;
}

export interface AppUrlTestConfigPatch {
  toleranceMs?: number;
}
