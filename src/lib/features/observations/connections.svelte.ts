import type { GuiConnectionItem } from '$lib/types/gui-api';

export type ConnectionDelta =
  | { type: 'started'; connection: GuiConnectionItem }
  | { type: 'updated'; connection: GuiConnectionItem }
  | { type: 'completed'; connection: GuiConnectionItem }
  | { type: 'snapshot'; connections: GuiConnectionItem[] };

/** Owns the bounded live connection projection, independent of IPC and shell UI. */
export class ConnectionObservations {
  activeConnections = $state<GuiConnectionItem[]>([]);
  connectionHistory = $state<GuiConnectionItem[]>([]);
  private _deltaSeq = $state(0);
  private _pendingDeltas: ConnectionDelta[] = [];
  private _runtimeId: string | null = null;

  get deltaSeq() { return this._deltaSeq; }

  bindRuntime(runtimeId: string) {
    if (runtimeId !== this._runtimeId) {
      this.reset();
      this._runtimeId = runtimeId;
    }
  }

  reset() {
    this._runtimeId = null;
    this.activeConnections = [];
    this.connectionHistory = [];
    this._pendingDeltas = [{ type: 'snapshot', connections: [] }];
    this._deltaSeq++;
  }

  drainDeltas(): ConnectionDelta[] {
    const deltas = this._pendingDeltas;
    this._pendingDeltas = [];
    return deltas;
  }

  apply(delta: ConnectionDelta) {
    if (!this._project(delta)) return;
    this._pendingDeltas.push(delta);
    if (this._pendingDeltas.length > 2_000) {
      // Consumers recover from the complete projection instead of silently
      // losing arbitrary deltas when a background page cannot keep up.
      this._pendingDeltas = [{ type: 'snapshot', connections: this.activeConnections }];
    }
    this._deltaSeq++;
  }

  private _project(delta: ConnectionDelta): boolean {
    if (delta.type === 'snapshot') {
      this.activeConnections = delta.connections.slice(0, 500);
      return true;
    }

    if (delta.type === 'completed') {
      const active = this.activeConnections.find((item) => item.flowId === delta.connection.flowId);
      const previous = active ?? this.connectionHistory.find((item) => item.flowId === delta.connection.flowId);
      if (previous && isOlderRevision(previous, delta.connection)) return false;
      this.activeConnections = this.activeConnections.filter((item) => item.flowId !== delta.connection.flowId);
      this.connectionHistory = [
        delta.connection,
        ...this.connectionHistory.filter((item) => item.flowId !== delta.connection.flowId),
      ].slice(0, 500);
      return true;
    }

    const closed = this.connectionHistory.find((item) => item.flowId === delta.connection.flowId);
    if (closed) return false;
    const index = this.activeConnections.findIndex((item) => item.flowId === delta.connection.flowId);
    const current = index >= 0 ? this.activeConnections[index] : undefined;
    if (current && isOlderRevision(current, delta.connection)) return false;
    const connection = mergeGuiConnection(current, delta.connection);
    this.activeConnections = [
      connection,
      ...this.activeConnections.filter((item) => item.flowId !== connection.flowId),
    ].slice(0, 500);
    return true;
  }

}

function isOlderRevision(current: GuiConnectionItem, incoming: GuiConnectionItem): boolean {
  return current.revision !== undefined
    && incoming.revision !== undefined
    && incoming.revision < current.revision;
}

function mergeGuiConnection(
  current: GuiConnectionItem | undefined,
  incoming: GuiConnectionItem,
): GuiConnectionItem {
  if (!current) return incoming;
  const merged = { ...current } as Record<string, unknown>;
  for (const [key, value] of Object.entries(incoming)) {
    if (value !== undefined) merged[key] = value;
  }
  if (incoming.selectionChain.length === 0) merged['selectionChain'] = current.selectionChain;
  if (incoming.relayChain.length === 0) merged['relayChain'] = current.relayChain;
  return merged as unknown as GuiConnectionItem;
}

export const connectionObservations = new ConnectionObservations();
