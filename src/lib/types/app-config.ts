// Mirror of Rust models::app_config

export interface AppConfig {
  schemaVersion: string;
  core: AppCoreConfig;
  logs: AppLogConfig;
  ui: AppUiConfig;
  localProxy: AppLocalProxyConfig;
  tun: AppTunConfig;
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
  defaultRoute?: string;
}

export interface AppLocalProxyConfig {
  host: string;
  port: number;
  sourceProxyConfigId?: string;
  bypass: string[];
}

export interface AppTunConfig {
  name?: string;
  addr: string;
  mask: string;
  secondaryAddr?: string;
  tag: string;
  mtu: number;
  dualStack: boolean;
  dnsHijack: boolean;
}

export interface AppRoutingConfig {
  injectCommonRules: boolean;
}

export interface AppUrlTestConfig {
  toleranceMs: number;
}

// Patch types for partial updates

export interface AppConfigPatch {
  core?: AppCoreConfigPatch;
  logs?: AppLogConfigPatch;
  ui?: AppUiConfigPatch;
  localProxy?: AppLocalProxyConfigPatch;
  tun?: AppTunConfigPatch;
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
  defaultRoute?: string | null;
}

export interface AppLocalProxyConfigPatch {
  host?: string;
  port?: number;
  sourceProxyConfigId?: string | null;
  bypass?: string[];
}

export interface AppTunConfigPatch {
  name?: string | null;
  addr?: string;
  mask?: string;
  secondaryAddr?: string | null;
  tag?: string;
  mtu?: number;
  dualStack?: boolean;
  dnsHijack?: boolean;
}

export interface AppRoutingConfigPatch {
  injectCommonRules?: boolean;
}

export interface AppUrlTestConfigPatch {
  toleranceMs?: number;
}
