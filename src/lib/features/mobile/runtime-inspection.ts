import { invoke } from '@tauri-apps/api/core';

export type RuntimeInspection = {
  status: 'confirmed'; transport: 'pinned-ipc'; coreInstanceId: string;
  configRevision: string; vpnObservation: 'unprovided';
  receipt: {operationId: string; profileId: string; state: 'confirmed'; mutation: false};
};

export function parseRuntimeInspection(value: unknown): RuntimeInspection {
  const result = value as RuntimeInspection | null;
  const receipt = result?.receipt as (RuntimeInspection['receipt'] & {coreInstanceId?: string; configRevision?: string}) | undefined;
  if (result?.status !== 'confirmed' || result.transport !== 'pinned-ipc' || result.vpnObservation !== 'unprovided'
    || typeof result.coreInstanceId !== 'string' || !result.coreInstanceId
    || typeof result.configRevision !== 'string' || !/^(0|[1-9]\d*)$/.test(result.configRevision) || BigInt(result.configRevision)>18446744073709551615n
    || receipt?.state !== 'confirmed' || receipt.mutation !== false
    || typeof receipt.operationId !== 'string' || !receipt.operationId || typeof receipt.profileId !== 'string' || !receipt.profileId
    || receipt.coreInstanceId !== result.coreInstanceId || receipt.configRevision !== result.configRevision) {
    throw new Error('控制通道回执未确认或所属实例不一致，请重新核对。');
  }
  return result;
}

export async function inspectRuntimeBridge(): Promise<RuntimeInspection> {
  return parseRuntimeInspection(await invoke('gui_runtime_bridge_inspect'));
}
