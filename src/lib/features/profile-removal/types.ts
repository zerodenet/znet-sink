export type RemovalAction = { kind: 'inactive' } | { kind: 'replace'; replacementId: string } | { kind: 'stop' };
export interface RemovalRuntime { state: 'running' | 'offline' | 'external' | 'unknown' | 'platform_unknown'; coreInstanceId: string | null; configRevision: string | null; managedPid: number | null }
export interface RemovalPreview {
  targetId: string; targetName: string; targetVersion: string; active: boolean; sourceType: string;
  guard: string; subscriptions: { id: string; name: string }[];
  replacements: { id: string; name: string; reason: string | null }[];
  runtime: RemovalRuntime; blockedReason: string | null; pending: boolean; overrideCount: number;
}
export interface RemovalInput { targetId: string; guard: string; action: RemovalAction }
export interface RemovalResult { status: 'confirmed' | 'unconfirmed' | 'not_deleted'; message: string; removedId: string | null; replacementId: string | null; runtimeStopped: boolean }
export interface RemovalGateway { preview(id: string): Promise<RemovalPreview>; commit(input: RemovalInput): Promise<RemovalResult>; reconcile(id: string): Promise<RemovalResult> }
export function removalInput(preview: RemovalPreview, action: RemovalAction): RemovalInput | null {
  if (preview.blockedReason || preview.pending) return null;
  if (!preview.active && action.kind !== 'inactive' || preview.active && action.kind === 'inactive') return null;
  if (action.kind === 'replace' && !preview.replacements.some(r => r.id === action.replacementId && !r.reason)) return null;
  return { targetId: preview.targetId, guard: preview.guard, action: structuredClone(action) };
}
export function removalConfirmed(input: RemovalInput, result: RemovalResult): boolean {
  return result.status === 'confirmed' && result.removedId === input.targetId
    && (input.action.kind !== 'replace' || result.replacementId === input.action.replacementId)
    && (input.action.kind !== 'stop' || result.runtimeStopped);
}
