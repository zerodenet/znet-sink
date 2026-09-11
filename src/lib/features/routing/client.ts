import { invoke } from '@tauri-apps/api/core';
import type { TraceRouteResult } from '$lib/types/diagnostics';
export async function guiTraceRoute(
  target: string,
  port?: number,
  protocol?: string,
  inboundTag?: string,
): Promise<TraceRouteResult> {
  return invoke<TraceRouteResult>('gui_trace_route', { target, port, protocol, inboundTag });
}
