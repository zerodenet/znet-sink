import type { LogEntry } from '../types/logs';

export function logFields(log: LogEntry): Record<string, unknown> | null {
  return log.fields && typeof log.fields === 'object' && !Array.isArray(log.fields)
    ? log.fields as Record<string, unknown> : null;
}

export function rawLogMessage(log: LogEntry): string {
  const message = logFields(log)?.message;
  return typeof message === 'string' && message.length > 0 ? message : log.message;
}

export function logHeadline(log: LogEntry): string {
  const message = rawLogMessage(log);
  // Only known lifecycle messages are translated; arbitrary protocol errors stay intact.
  if (log.source !== 'core') return message;
  return ({ 'session accepted': '接收连接', 'session finished': '连接结束', 'session failed': '连接失败' } as Record<string, string>)[message] ?? message;
}

export function logContext(log: LogEntry): string[] {
  const fields = logFields(log);
  if (!fields) return [];
  const text = (key: string): string | undefined => {
    const value = fields[key];
    return typeof value === 'string' && value.trim() ? value : undefined;
  };
  if (log.source === 'plugin') {
    return [text('pluginId'), text('componentId'), text('action')].filter((value): value is string => !!value);
  }
  const result: string[] = [];
  const target = text('target');
  if (target) {
    const host = /^Domain\("(.*)"\)$/.exec(target)?.[1] ?? target;
    const port = fields.port;
    result.push(`目标 ${host}${typeof port === 'number' && Number.isInteger(port) && port > 0 && port <= 65535 ? `:${port}` : ''}`);
  }
  const outbound = text('outbound_tag');
  if (outbound) result.push(`出口 ${outbound}`);
  const error = text('error');
  if (error) result.push(error);
  else if (typeof fields.duration_ms === 'number' && Number.isFinite(fields.duration_ms) && fields.duration_ms >= 0) {
    result.push(`耗时 ${fields.duration_ms < 1000 ? `${fields.duration_ms} ms` : `${(fields.duration_ms / 1000).toFixed(1)} s`}`);
  }
  return result;
}
