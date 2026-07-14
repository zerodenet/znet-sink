// Mirror of Rust models::logs

export type LogSource = 'app' | 'core';

export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error';

export interface LogEntry {
  id: number;
  source: LogSource;
  level: LogLevel;
  message: string;
  fields?: unknown;
  occurredAtUnixMs: number;
}

export interface LogAppend {
  source: LogSource;
  level: LogLevel;
  message: string;
  fields?: unknown;
}

export interface LogQuery {
  source?: LogSource;
  level?: LogLevel;
  limit?: number;
  beforeId?: number;
}

export interface LogPage {
  items: LogEntry[];
  hasMore: boolean;
  oldestAvailableId?: number;
}
