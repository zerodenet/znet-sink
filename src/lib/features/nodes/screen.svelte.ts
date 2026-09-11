import type { NodeScreenSnapshot } from '$lib/types/gui-api';
import { shouldApplyNodeScreenSnapshot } from '$lib/features/probes/reconcile';
/** Shared node browsing and passive observations; no manual execution. */
export class NodeScreenState {
  nodeScreen = $state<NodeScreenSnapshot | null>(null);
  error = $state<string | null>(null);
  disposed = false;
  private sequence = 0;
  private applied = 0;
  private listeners = new Set<() => void>();
  private query: (reason: string) => Promise<NodeScreenSnapshot>;
  constructor(query: (reason: string) => Promise<NodeScreenSnapshot>) { this.query = query; }
  async refresh(reason: string) {
    if (this.disposed) return;
    const request = ++this.sequence;
    try {
      const snapshot = await this.query(reason);
      if (this.disposed || !shouldApplyNodeScreenSnapshot({currentRevision: this.nodeScreen?.revision, candidateRevision: snapshot.revision, requestSequence: request, lastAppliedRequest: this.applied})) return;
      this.applied = request; this.nodeScreen = snapshot; this.error = null;
    } catch (error) {
      if (!this.disposed && request >= this.applied) { this.applied = request; this.error = error instanceof Error ? error.message : String(error); }
    }
  }
  async attach(registration: Promise<() => void>) {
    try {
      const unlisten = await registration;
      if (this.disposed) unlisten(); else this.listeners.add(unlisten);
    } catch (error) { if (!this.disposed) this.error = error instanceof Error ? error.message : String(error); }
  }
  dispose() {
    this.disposed = true;
    for (const unlisten of this.listeners) { try { unlisten(); } catch { /* Continue releasing remaining listeners. */ } }
    this.listeners.clear();
  }
}
