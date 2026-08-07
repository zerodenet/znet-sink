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
  const recentCandidates = mergeConnectionLists(recentEvents, recentSnapshot);
  const recentById = new Map(recentCandidates.map((connection) => [connection.flowId, connection]));

  const active = activeCandidates
    .filter((connection) => {
      const completed = recentById.get(connection.flowId);
      return !completed || activeRepresentsNewLifetime(connection, completed);
    })
    .sort((left, right) => connectionTimestamp(right) - connectionTimestamp(left))
    .slice(0, limit);

  const activeById = new Map(active.map((connection) => [connection.flowId, connection]));
  const recent = recentCandidates
    .filter((connection) => {
      const current = activeById.get(connection.flowId);
      return !current || !activeRepresentsNewLifetime(current, connection);
    })
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
): GuiConnectionItem[] {
  const merged = new Map<string, GuiConnectionItem>();

  for (const connection of fallback) {
    merged.set(connection.flowId, connection);
  }
  for (const connection of primary) {
    const current = merged.get(connection.flowId);
    merged.set(connection.flowId, mergeConnection(current, connection));
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
