import { invoke } from '@tauri-apps/api/core';
import type { CoreStatus, LogEntry, Capability } from '$lib/types/core';

export type { CoreStatus, LogEntry, Capability };

// 内核进程
export async function getCoreStatus(): Promise<CoreStatus> {
  return invoke('core_status');
}

export async function startCore(): Promise<void> {
  return invoke('core_process_start');
}

export async function stopCore(): Promise<void> {
  return invoke('core_process_stop');
}

// 内核配置
export async function getCoreConfig(): Promise<Record<string, unknown>> {
  return invoke('core_config_get');
}

// 应用配置
export async function getAppConfig(): Promise<Record<string, unknown>> {
  return invoke('app_config_get');
}

export async function updateAppConfig(config: Record<string, unknown>): Promise<void> {
  return invoke('app_config_update', { config });
}

// 日志
export async function getLogs(): Promise<LogEntry[]> {
  return invoke('logs_list');
}

export async function clearLogs(): Promise<void> {
  return invoke('logs_clear');
}

// 能力快照
export async function getCapabilities(): Promise<Capability[]> {
  return invoke('gui_capabilities_snapshot');
}
