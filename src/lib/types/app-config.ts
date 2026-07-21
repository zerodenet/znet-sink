// Mirror of Rust models::app_config

export interface AppConfig {
  schemaVersion: string;
  core: AppCoreConfig;
  logs: AppLogConfig;
  ui: AppUiConfig;
  localProxy: AppLocalProxyConfig;
  tun: AppTunConfig;
  routing: AppRoutingConfig;
}

export interface AppCoreConfig {
  kernel: string;
  autoConnect: boolean;
  autoStart: boolean;
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
}

export interface AppTunConfig {
  name?: string;
  addr: string;
  tag: string;
  mtu: number;
}

export interface AppRoutingConfig {
  injectCommonRules: boolean;
}

// Patch types for partial updates

export interface AppConfigPatch {
  core?: AppCoreConfigPatch;
  logs?: AppLogConfigPatch;
  ui?: AppUiConfigPatch;
  localProxy?: AppLocalProxyConfigPatch;
  tun?: AppTunConfigPatch;
  routing?: AppRoutingConfigPatch;
}

export interface AppCoreConfigPatch {
  kernel?: string;
  autoConnect?: boolean;
  autoStart?: boolean;
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
}

export interface AppTunConfigPatch {
  name?: string | null;
  addr?: string;
  tag?: string;
  mtu?: number;
}

export interface AppRoutingConfigPatch {
  injectCommonRules?: boolean;
}
