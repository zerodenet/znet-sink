import type { DebugFrame } from '$lib/types/debug';
import type { GuiConnectionItem } from '$lib/types/gui-api';

export type ConnectionWireSource = 'event' | 'active_flows' | 'recent_flows';

export interface ConnectionWireMetadata {
  rawSource: ConnectionWireSource;
  rawPayload: unknown;
  rawEnvelope?: unknown;
  eventType?: string;
  eventId?: string;
  eventSequence?: number;
  eventOccurredAtUnixMs?: number;
  capturedAtUnixMs?: number;
  startedAtUnixMs?: number;
  endedAtUnixMs?: number;
}

export type ConnectionWireIndex = Record<string, ConnectionWireMetadata[]>;
export type WireConnection = GuiConnectionItem & Partial<ConnectionWireMetadata>;

export interface ConnectionWireInput {
  activeResponse?: unknown;
  recentResponse?: unknown;
  eventFrames?: DebugFrame[];
}

export function buildConnectionWireIndex({
  activeResponse,
  recentResponse,
  eventFrames = [],
}: ConnectionWireInput): ConnectionWireIndex {
  const index: ConnectionWireIndex = {};

  collectQueryRecords(index, activeResponse, 'active_flows');
  collectQueryRecords(index, recentResponse, 'recent_flows');

  for (const frame of [...eventFrames].sort((left, right) => left.id - right.id)) {
    collectEventRecords(index, frame);
  }

  return index;
}

export function attachConnectionWireMetadata(
  connection: GuiConnectionItem,
  index: ConnectionWireIndex,
): WireConnection {
  const metadata = selectConnectionWireMetadata(connection, index[connection.flowId] ?? []);
  return metadata ? { ...connection, ...metadata } : connection;
}

export function selectConnectionWireMetadata(
  connection: GuiConnectionItem,
  candidates: ConnectionWireMetadata[],
): ConnectionWireMetadata | undefined {
  if (candidates.length === 0) return undefined;

  return [...candidates].sort((left, right) => {
    const scoreDiff = metadataScore(connection, right) - metadataScore(connection, left);
    if (scoreDiff !== 0) return scoreDiff;
    return metadataTimestamp(right) - metadataTimestamp(left);
  })[0];
}

export function mergeConnectionWireIndexes(
  current: ConnectionWireIndex,
  incoming: ConnectionWireIndex,
  limitPerFlow = 20,
): ConnectionWireIndex {
  const merged: ConnectionWireIndex = { ...current };

  for (const [flowId, records] of Object.entries(incoming)) {
    const unique = new Map<string, ConnectionWireMetadata>();
    for (const record of [...(merged[flowId] ?? []), ...records]) {
      unique.set(metadataIdentity(record), record);
    }
    merged[flowId] = [...unique.values()]
      .sort((left, right) => metadataTimestamp(left) - metadataTimestamp(right))
      .slice(-limitPerFlow);
  }

  return merged;
}

function collectQueryRecords(
  index: ConnectionWireIndex,
  response: unknown,
  source: 'active_flows' | 'recent_flows',
) {
  const payload = unwrapQueryResponse(response, source);
  for (const record of extractConnectionRecords(payload)) {
    const flowId = flowIdFrom(record);
    if (!flowId) continue;
    pushMetadata(index, flowId, {
      rawSource: source,
      rawPayload: record,
      startedAtUnixMs: timestampFrom(record, 'started'),
      endedAtUnixMs: timestampFrom(record, 'ended'),
    });
  }
}

function collectEventRecords(index: ConnectionWireIndex, frame: DebugFrame) {
  if (frame.frameType !== 'event') return;
  const envelope = objectValue(frame.payload);
  if (!envelope) return;

  const eventType = stringValue(envelope, ['event_type', 'eventType', 'type']);
  if (!eventType?.startsWith('flow.')) return;

  const eventId = stringValue(envelope, ['event_id', 'eventId']);
  const eventSequence = numberValue(envelope, ['sequence']);
  const eventOccurredAtUnixMs = numberValue(envelope, [
    'occurred_at_unix_ms',
    'occurredAtUnixMs',
  ]);
  const payload = envelope['payload'] ?? envelope;
  const records = eventType === 'flow.snapshot'
    ? extractConnectionRecords(payload)
    : extractConnectionRecords(payload, true);

  for (const record of records) {
    const flowId = flowIdFrom(record);
    if (!flowId) continue;
    pushMetadata(index, flowId, {
      rawSource: 'event',
      rawPayload: record,
      rawEnvelope: eventType === 'flow.snapshot' ? undefined : frame.payload,
      eventType,
      eventId,
      eventSequence,
      eventOccurredAtUnixMs,
      capturedAtUnixMs: frame.atMs,
      startedAtUnixMs: timestampFrom(record, 'started')
        ?? (eventType === 'flow.started' ? eventOccurredAtUnixMs : undefined),
      endedAtUnixMs: timestampFrom(record, 'ended')
        ?? (eventType === 'flow.completed' ? eventOccurredAtUnixMs : undefined),
    });
  }
}

function pushMetadata(index: ConnectionWireIndex, flowId: string, metadata: ConnectionWireMetadata) {
  const items = index[flowId] ?? [];
  items.push(metadata);
  index[flowId] = items.slice(-20);
}

function unwrapQueryResponse(response: unknown, variant: string): unknown {
  let current: unknown = response;

  for (let depth = 0; depth < 4; depth++) {
    const object = objectValue(current);
    if (!object) break;
    if (variant in object) return object[variant];
    if ('response' in object) {
      current = object['response'];
      continue;
    }
    if ('result' in object) {
      current = object['result'];
      continue;
    }
    break;
  }

  const object = objectValue(current);
  return object?.[variant] ?? current;
}

function extractConnectionRecords(value: unknown, includeDirect = false): unknown[] {
  if (Array.isArray(value)) {
    return value.flatMap((item) => extractConnectionRecords(item, true));
  }

  const object = objectValue(value);
  if (!object) return [];

  const record = objectValue(object['record']);
  if (record && flowIdFrom(record)) return [record];
  if (includeDirect && flowIdFrom(object)) return [object];

  for (const key of ['records', 'items', 'flows', 'connections', 'data', 'active', 'recent']) {
    if (key in object) {
      const records = extractConnectionRecords(object[key], true);
      if (records.length > 0) return records;
    }
  }

  if (flowIdFrom(object)) return [object];
  return [];
}

function metadataScore(connection: GuiConnectionItem, metadata: ConnectionWireMetadata): number {
  let score = metadata.rawSource === 'event' ? 20 : 10;
  const connectionStarted = connection.startedAtUnixMs;
  const connectionEnded = connection.endedAtUnixMs;

  if (connectionStarted !== undefined && metadata.startedAtUnixMs !== undefined) {
    const delta = Math.abs(connectionStarted - metadata.startedAtUnixMs);
    if (delta <= 1) score += 100;
    else if (delta <= 1_000) score += 60;
    else score -= Math.min(40, Math.floor(delta / 1_000));
  }

  if (connectionEnded !== undefined && metadata.endedAtUnixMs !== undefined) {
    const delta = Math.abs(connectionEnded - metadata.endedAtUnixMs);
    if (delta <= 1) score += 100;
    else if (delta <= 1_000) score += 60;
  }

  const connectionCompleted = connection.state === 'completed' || connectionEnded !== undefined;
  const metadataCompleted = metadata.eventType === 'flow.completed'
    || metadata.rawSource === 'recent_flows'
    || metadata.endedAtUnixMs !== undefined;
  if (connectionCompleted === metadataCompleted) score += 30;
  else score -= 30;

  return score;
}

function metadataIdentity(metadata: ConnectionWireMetadata): string {
  if (metadata.eventId) return `event-id:${metadata.eventId}`;
  if (metadata.eventSequence !== undefined) {
    return `event-sequence:${metadata.eventType ?? ''}:${metadata.eventSequence}`;
  }
  return [
    metadata.rawSource,
    metadata.eventType ?? '',
    metadata.startedAtUnixMs ?? '',
    metadata.endedAtUnixMs ?? '',
    stableSerialize(metadata.rawPayload),
  ].join(':');
}

function metadataTimestamp(metadata: ConnectionWireMetadata): number {
  return metadata.eventOccurredAtUnixMs
    ?? metadata.capturedAtUnixMs
    ?? metadata.endedAtUnixMs
    ?? metadata.startedAtUnixMs
    ?? 0;
}

function stableSerialize(value: unknown): string {
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function timestampFrom(value: unknown, kind: 'started' | 'ended'): number | undefined {
  const object = objectValue(value);
  if (!object) return undefined;
  const timing = objectValue(object['timing']) ?? object;
  return kind === 'started'
    ? numberValue(timing, ['started_at_unix_ms', 'startedAtUnixMs', 'started_at'])
    : numberValue(timing, [
        'ended_at_unix_ms',
        'endedAtUnixMs',
        'finished_at_unix_ms',
        'finishedAtUnixMs',
      ]);
}

function flowIdFrom(value: unknown): string | undefined {
  const object = objectValue(value);
  return object ? stringValue(object, ['flow_id', 'flowId', 'id', 'connection_id', 'connectionId']) : undefined;
}

function objectValue(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

function stringValue(object: Record<string, unknown>, keys: string[]): string | undefined {
  for (const key of keys) {
    const value = object[key];
    if (typeof value === 'string') return value;
  }
  return undefined;
}

function numberValue(object: Record<string, unknown>, keys: string[]): number | undefined {
  for (const key of keys) {
    const value = object[key];
    if (typeof value === 'number' && Number.isFinite(value)) return value;
  }
  return undefined;
}
