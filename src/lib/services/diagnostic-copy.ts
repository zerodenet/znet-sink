import type { DebugFrame } from '$lib/types/debug';
import type { LogEntry, LogLevel, LogSource } from '$lib/types/logs';

export interface LogCopyContext {
  source: LogSource | 'all';
  level: LogLevel | 'all';
  search?: string;
  hasMore: boolean;
  copiedAtUnixMs?: number;
}

export interface DebugFrameCopyContext {
  frameType: string;
  hasMore: boolean;
  copiedAtUnixMs?: number;
}

function logMessage(log: LogEntry): string {
  if (log.fields && typeof log.fields === 'object' && !Array.isArray(log.fields)) {
    const message = (log.fields as Record<string, unknown>)['message'];
    if (typeof message === 'string' && message.length > 0) return message;
  }
  return log.message;
}

export function logCopyRecord(log: LogEntry): Record<string, unknown> {
  return {
    id: log.id,
    occurredAtUnixMs: log.occurredAtUnixMs,
    occurredAt: new Date(log.occurredAtUnixMs).toISOString(),
    source: log.source,
    level: log.level,
    message: logMessage(log),
    fields: log.fields,
  };
}

export function serializeLogForClipboard(log: LogEntry): string {
  return JSON.stringify(logCopyRecord(log), null, 2);
}

export function serializeLogsForClipboard(
  logs: LogEntry[],
  context: LogCopyContext,
): string {
  return JSON.stringify({
    schemaId: 'znet.clipboard.logs.v1',
    copiedAtUnixMs: context.copiedAtUnixMs ?? Date.now(),
    source: context.source,
    level: context.level,
    search: context.search,
    count: logs.length,
    partial: context.hasMore,
    items: logs.map(logCopyRecord),
  }, null, 2);
}

export function debugFrameCopyRecord(frame: DebugFrame): Record<string, unknown> {
  return {
    id: frame.id,
    atMs: frame.atMs,
    occurredAt: new Date(frame.atMs).toISOString(),
    direction: frame.direction,
    frameType: frame.frameType,
    elapsedMs: frame.elapsedMs,
    error: frame.error,
    payload: frame.payload,
  };
}

export function serializeDebugFrameForClipboard(frame: DebugFrame): string {
  return JSON.stringify(debugFrameCopyRecord(frame), null, 2);
}

export function serializeDebugFramesForClipboard(
  frames: DebugFrame[],
  context: DebugFrameCopyContext,
): string {
  return JSON.stringify({
    schemaId: 'znet.clipboard.ipc-debug.v1',
    copiedAtUnixMs: context.copiedAtUnixMs ?? Date.now(),
    frameType: context.frameType,
    count: frames.length,
    partial: context.hasMore,
    items: frames.map(debugFrameCopyRecord),
  }, null, 2);
}
