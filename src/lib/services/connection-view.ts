import type { ConnectionWireMetadata } from '$lib/services/connection-wire';
import type { GuiConnectionItem } from '$lib/types/gui-api';

export type ConnectionOrigin = 'active' | 'recent';

export type DisplayConnection = Omit<GuiConnectionItem, 'source'>
  & Partial<ConnectionWireMetadata>
  & {
    source: string;
    origin: ConnectionOrigin;
    protocol: string;
  };

export interface ConnectionViewInput {
  activeSnapshot: GuiConnectionItem[];
  recentSnapshot: GuiConnectionItem[];
  activeEvents: GuiConnectionItem[];
  recentEvents: GuiConnectionItem[];
  limit?: number;
}

export interface ConnectionView {
  active: DisplayConnection[];
  recent: DisplayConnection[];
}

const DEFAULT_LIMIT = 500;

export function buildConnectionView({
  activeSnapshot,
  recentSnapshot,
  activeEvents,
  recentEvents,
  limit = DEFAULT_LIMIT,
}: ConnectionViewInput): ConnectionView {
  const activeCandidates = mergeConnectionLists(activeEvents, activeSnapshot);
  const recentCandidates = mergeConnectionLists(
    recentEvents,
    recentSnapshot,
    connectionLifecycleKey,
  );
  const latestCompletedById = latestCompletedConnections(recentCandidates);

  const active = activeCandidates
    .filter((connection) => {
      const completed = latestCompletedById.get(connection.flowId);
      return !completed || activeRepresentsNewLifetime(connection, completed);
    })
    .sort((left, right) => connectionTimestamp(right) - connectionTimestamp(left))
    .slice(0, limit);

  // Completed lifetimes belong to client history. A currently active flow with
  // a reused numeric ID must not erase an older completed lifetime.
  const recent = recentCandidates
    .sort((left, right) => completedTimestamp(right) - completedTimestamp(left))
    .slice(0, limit);

  return {
    active: active.map((connection) => toDisplayConnection(connection, 'active')),
    recent: recent.map((connection) => toDisplayConnection(connection, 'recent')),
  };
}

export function toDisplayConnection(
  connection: GuiConnectionItem,
  origin: ConnectionOrigin,
): DisplayConnection {
  return {
    ...connection,
    source: connection.source ?? '-',
    origin,
    protocol: connection.network,
  } as DisplayConnection;
}

export function mergeConnectionLists(
  primary: GuiConnectionItem[],
  fallback: GuiConnectionItem[],
  keyOf: (connection: GuiConnectionItem) => string = (connection) => connection.flowId,
): GuiConnectionItem[] {
  const merged = new Map<string, GuiConnectionItem>();

  for (const connection of fallback) {
    merged.set(keyOf(connection), connection);
  }
  for (const connection of primary) {
    const key = keyOf(connection);
    const current = merged.get(key);
    merged.set(key, mergeConnection(current, connection));
  }

  return [...merged.values()];
}

export function compareConnectionFreshness(
  left: GuiConnectionItem,
  right: GuiConnectionItem,
): number {
  if (left.revision !== undefined && right.revision !== undefined && left.revision !== right.revision) {
    return left.revision - right.revision;
  }
  return connectionTimestamp(left) - connectionTimestamp(right);
}

export function connectionLifecycleKey(connection: GuiConnectionItem): string {
  return [
    connection.flowId,
    connection.startedAtUnixMs ?? '',
    connection.endedAtUnixMs ?? '',
  ].join(':');
}

function latestCompletedConnections(
  connections: GuiConnectionItem[],
): Map<string, GuiConnectionItem> {
  const latest = new Map<string, GuiConnectionItem>();

  for (const connection of connections) {
    const current = latest.get(connection.flowId);
    if (!current || completedTimestamp(connection) >= completedTimestamp(current)) {
      latest.set(connection.flowId, connection);
    }
  }

  return latest;
}

function mergeConnection(
  current: GuiConnectionItem | undefined,
  incoming: GuiConnectionItem,
): GuiConnectionItem {
  if (!current) return incoming;

  const incomingIsNewer = compareConnectionFreshness(incoming, current) >= 0;
  const newer = incomingIsNewer ? incoming : current;
  const older = incomingIsNewer ? current : incoming;
  const merged = { ...older, ...newer };

  if (newer.selectionChain.length === 0 && older.selectionChain.length > 0) {
    merged.selectionChain = older.selectionChain;
  }
  if (newer.relayChain.length === 0 && older.relayChain.length > 0) {
    merged.relayChain = older.relayChain;
  }

  return merged;
}

function activeRepresentsNewLifetime(
  active: GuiConnectionItem,
  completed: GuiConnectionItem,
): boolean {
  const activeStartedAt = active.startedAtUnixMs ?? 0;
  const completedAt = completed.endedAtUnixMs ?? completed.updatedAtUnixMs ?? 0;

  if (activeStartedAt > 0 && completedAt > 0 && activeStartedAt > completedAt) {
    return true;
  }

  if (active.revision !== undefined && completed.revision !== undefined) {
    return active.revision > completed.revision && active.state !== 'completed';
  }

  return false;
}

function connectionTimestamp(connection: GuiConnectionItem): number {
  return connection.updatedAtUnixMs
    ?? connection.lastActivityAtUnixMs
    ?? connection.endedAtUnixMs
    ?? connection.startedAtUnixMs
    ?? 0;
}

function completedTimestamp(connection: GuiConnectionItem): number {
  return connection.endedAtUnixMs
    ?? connection.updatedAtUnixMs
    ?? connection.lastActivityAtUnixMs
    ?? connection.startedAtUnixMs
    ?? 0;
}
