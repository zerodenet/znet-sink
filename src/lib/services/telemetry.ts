import { appendLog } from '$lib/services/core';

export type TelemetryLevel = 'debug' | 'info' | 'warn' | 'error';
export type TelemetryArea = 'startup' | 'kernel' | 'ipc' | 'proxy' | 'config' | 'subscription' | 'update' | 'ui';

export interface TelemetryEvent {
  level: TelemetryLevel;
  area: TelemetryArea;
  operation: string;
  message: string;
  code?: string;
  durationMs?: number;
  correlationId?: string;
  context?: Record<string, unknown>;
}

function safeContext(context?: Record<string, unknown>): Record<string, unknown> | undefined {
  if (!context) return undefined;
  return Object.fromEntries(Object.entries(context).map(([key, value]) => {
    const normalized = key.toLowerCase();
    if (['password', 'secret', 'token', 'authorization', 'content'].some((part) => normalized.includes(part))) {
      return [key, '[redacted]'];
    }
    if (typeof value === 'string' && value.length > 500) return [key, `${value.slice(0, 500)}…`];
    return [key, value];
  }));
}

export function createCorrelationId(prefix = 'op'): string {
  const random = globalThis.crypto?.randomUUID?.() ?? Math.random().toString(36).slice(2);
  return `${prefix}-${Date.now().toString(36)}-${random.slice(0, 8)}`;
}

export async function recordTelemetry(event: TelemetryEvent): Promise<void> {
  try {
    await appendLog({
      source: 'app',
      level: event.level,
      message: event.message,
      fields: {
        schema: 'znet.telemetry.v1',
        area: event.area,
        operation: event.operation,
        code: event.code,
        durationMs: event.durationMs,
        correlationId: event.correlationId,
        context: safeContext(event.context),
      },
    });
  } catch (error) {
    console.error('[telemetry] failed to persist event', event, error);
  }
}

export async function tracedOperation<T>(
  area: TelemetryArea,
  operation: string,
  action: (correlationId: string) => Promise<T>,
  context?: Record<string, unknown>,
): Promise<T> {
  const correlationId = createCorrelationId(operation.replaceAll('.', '-'));
  const startedAt = performance.now();
  void recordTelemetry({ level: 'info', area, operation, message: `${operation} started`, correlationId, context });
  try {
    const result = await action(correlationId);
    void recordTelemetry({
      level: 'info',
      area,
      operation,
      message: `${operation} completed`,
      durationMs: Math.round(performance.now() - startedAt),
      correlationId,
      context,
    });
    return result;
  } catch (error) {
    const appError = error as { code?: string; message?: string };
    void recordTelemetry({
      level: 'error',
      area,
      operation,
      code: appError?.code,
      message: appError?.message || `${operation} failed`,
      durationMs: Math.round(performance.now() - startedAt),
      correlationId,
      context,
    });
    throw error;
  }
}

let globalHandlersInstalled = false;

export function installGlobalErrorTelemetry(): () => void {
  if (globalHandlersInstalled) return () => {};
  globalHandlersInstalled = true;
  const onError = (event: ErrorEvent) => {
    void recordTelemetry({
      level: 'error',
      area: 'ui',
      operation: 'window.error',
      message: event.message || 'Unhandled frontend error',
      context: {
        filename: event.filename,
        line: event.lineno,
        column: event.colno,
        stack: event.error instanceof Error ? event.error.stack : undefined,
      },
    });
  };
  const onUnhandledRejection = (event: PromiseRejectionEvent) => {
    const reason = event.reason;
    void recordTelemetry({
      level: 'error',
      area: 'ui',
      operation: 'window.unhandledrejection',
      message: reason instanceof Error ? reason.message : String(reason),
      context: { stack: reason instanceof Error ? reason.stack : undefined },
    });
  };
  window.addEventListener('error', onError);
  window.addEventListener('unhandledrejection', onUnhandledRejection);
  return () => {
    window.removeEventListener('error', onError);
    window.removeEventListener('unhandledrejection', onUnhandledRejection);
    globalHandlersInstalled = false;
  };
}
