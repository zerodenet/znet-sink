export interface CoreStatus {
  running: boolean;
  pid?: number;
  uptime?: number;
  memory_usage?: number;
  connections?: number;
}

export interface CoreConfig {
  endpoint: string;
  log_level: string;
  tun_mode: boolean;
  auto_start: boolean;
}

export interface AppConfig {
  theme: 'light' | 'dark' | 'system';
  ui_mode: 'lite' | 'pro';
  core: CoreConfig;
}

export interface LogEntry {
  id: string;
  timestamp: number;
  level: 'debug' | 'info' | 'warn' | 'error';
  message: string;
  source: string;
}

export interface Capability {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  available: boolean;
}
