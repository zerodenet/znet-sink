import type { ConnectionWireMetadata } from '$lib/services/connection-wire';
import type { DebugFrame } from '$lib/types/debug';
import type { GuiConnectionItem } from '$lib/types/gui-api';

export type PersistedConnection = GuiConnectionItem & Partial<ConnectionWireMetadata>;

/**
 * Rebuild completed connection history from the GUI-owned IPC event journal.
 * This never queries the kernel. The journal is written when the IPC broadcast
 * frame is received, so completed records remain available across page and app
 * restarts while the kernel remains responsible only for live execution.
 */
export function buildPersistedConnectionHistory(
  frames: DebugFrame[],
  limit = 500,
): PersistedConnection[] {
  const records = new Map<string, PersistedConnection>();

  for (const frame of [...frames].sort((left, right) => left.id - right.id)) {
    if (frame.frameType !== 'event') continue;
    const envelope = objectValue(frame.payload);
    if (!envelope) continue;

    const eventType = text(envelope, ['event_type', 'eventType', 'type']);
    if (eventType !== 'flow.completed' && eventType !== 'connection.closed') continue;

    const payload = objectValue(envelope['payload']) ?? envelope;
    const rawRecord = objectValue(payload['record']) ?? payload;
    const connection = parseCompletedRecord(rawRecord);
    if (!connection) continue;

    const eventOccurredAtUnixMs = number(envelope, ['occurred_at_unix_ms', 'occurredAtUnixMs']);
    const enriched: PersistedConnection = {
      ...connection,
      rawSource: 'event',
      rawPayload: rawRecord,
      rawEnvelope: frame.payload,
      eventType,
      eventId: text(envelope, ['event_id', 'eventId']),
      eventSequence: number(envelope, ['sequence']),
      eventOccurredAtUnixMs,
      capturedAtUnixMs: frame.atMs,
    };

    records.set(connectionLifecycleKey(enriched), enriched);
  }

  return [...records.values()]
    .sort((left, right) => completedTimestamp(right) - completedTimestamp(left))
    .slice(0, limit);
}

function parseCompletedRecord(raw: Record<string, unknown>): GuiConnectionItem | null {
  const flowId = text(raw, ['flow_id', 'flowId', 'id']);
  if (!flowId) return null;

  const target = objectValue(raw['target']) ?? {};
  const inbound = objectValue(raw['inbound']) ?? {};
  const source = objectValue(raw['source']) ?? {};
  const route = objectValue(raw['route']) ?? {};
  const path = objectValue(raw['path']) ?? {};
  const outbound = objectValue(path['outbound']) ?? objectValue(raw['outbound']) ?? {};
  const remote = objectValue(path['remote']) ?? {};
  const traffic = objectValue(raw['traffic']) ?? raw;
  const throughput = objectValue(raw['throughput']) ?? raw;
  const timing = objectValue(raw['timing']) ?? raw;
  const result = objectValue(raw['result']) ?? {};
  const failure = objectValue(result['failure']) ?? {};
  const matchedRule = objectValue(route['matched_rule']) ?? objectValue(route['matchedRule']) ?? {};

  const targetHost = text(target, ['host', 'value'])
    ?? text(raw, ['destination', 'target_host', 'targetHost'])
    ?? '-';
  const targetPort = number(target, ['port']) ?? number(raw, ['port', 'target_port', 'targetPort']);
  const sourceIp = text(source, ['ip']) ?? text(raw, ['source_ip', 'sourceIp']);
  const sourcePort = number(source, ['port']) ?? number(raw, ['source_port', 'sourcePort']);

  return {
    flowId,
    revision: number(raw, ['revision']),
    state: text(raw, ['state']) ?? 'completed',
    network: text(raw, ['network', 'protocol']) ?? 'tcp',
    source: sourceIp ? endpoint(sourceIp, sourcePort) : undefined,
    sourceIp,
    sourcePort,
    processId: number(source, ['process_id', 'processId']) ?? number(raw, ['process_id', 'processId']),
    processName: text(source, ['process_name', 'processName']) ?? text(raw, ['process_name', 'processName']),
    processPath: text(source, ['process_path', 'processPath']) ?? text(raw, ['process_path', 'processPath']),
    destination: endpoint(targetHost, targetPort),
    targetHost,
    targetIp: text(target, ['resolved_ip', 'resolvedIp']),
    targetPort,
    sniffedHost: text(target, ['sniffed_host', 'sniffedHost']),
    inboundTag: text(inbound, ['tag']) ?? text(raw, ['inbound_tag', 'inboundTag']),
    inboundProtocol: text(inbound, ['protocol']),
    outboundTag: text(outbound, ['tag']) ?? text(raw, ['outbound_tag', 'outboundTag']),
    outboundProtocol: text(outbound, ['protocol']),
    remoteDestination: text(remote, ['host'])
      ? endpoint(text(remote, ['host']) as string, number(remote, ['port']))
      : undefined,
    policyTag: text(raw, ['policy_tag', 'policyTag']),
    routeMode: text(route, ['mode']),
    routeAction: text(route, ['action']),
    matchedRuleIndex: number(matchedRule, ['index']),
    matchedRule: text(matchedRule, ['condition']),
    selectionChain: stringArray(route['selection_chain'] ?? route['selectionChain']),
    relayChain: endpointTagArray(path['relay_chain'] ?? path['relayChain']),
    outcome: text(result, ['outcome']) ?? text(raw, ['outcome']),
    closeReason: text(result, ['close_reason', 'closeReason']) ?? text(raw, ['close_reason', 'closeReason']),
    failureStage: text(failure, ['stage']),
    failureCode: text(failure, ['code']),
    failureMessage: text(failure, ['message']),
    bytesUp: number(traffic, ['bytes_up', 'bytesUp']) ?? 0,
    bytesDown: number(traffic, ['bytes_down', 'bytesDown']) ?? 0,
    inboundRxBytes: number(traffic, ['inbound_rx_bytes', 'inboundRxBytes']),
    inboundTxBytes: number(traffic, ['inbound_tx_bytes', 'inboundTxBytes']),
    outboundRxBytes: number(traffic, ['outbound_rx_bytes', 'outboundRxBytes']),
    outboundTxBytes: number(traffic, ['outbound_tx_bytes', 'outboundTxBytes']),
    throughputUpBps: number(throughput, ['upload_bps', 'uploadBps', 'throughput_up_bps', 'throughputUpBps']),
    throughputDownBps: number(throughput, ['download_bps', 'downloadBps', 'throughput_down_bps', 'throughputDownBps']),
    startedAtUnixMs: number(timing, ['started_at_unix_ms', 'startedAtUnixMs']),
    lastActivityAtUnixMs: number(timing, ['last_activity_at_unix_ms', 'lastActivityAtUnixMs']),
    endedAtUnixMs: number(timing, [
      'ended_at_unix_ms',
      'endedAtUnixMs',
      'finished_at_unix_ms',
      'finishedAtUnixMs',
    ]),
    updatedAtUnixMs: number(throughput, ['sampled_at_unix_ms', 'sampledAtUnixMs']),
    durationMs: number(timing, ['duration_ms', 'durationMs']),
  };
}

function connectionLifecycleKey(connection: GuiConnectionItem): string {
  return [
    connection.flowId,
    connection.startedAtUnixMs ?? '',
    connection.endedAtUnixMs ?? '',
  ].join(':');
}

function completedTimestamp(connection: GuiConnectionItem): number {
  return connection.endedAtUnixMs
    ?? connection.updatedAtUnixMs
    ?? connection.lastActivityAtUnixMs
    ?? connection.startedAtUnixMs
    ?? 0;
}

function endpoint(host: string, port?: number): string {
  if (port === undefined) return host;
  return host.includes(':') ? `[${host}]:${port}` : `${host}:${port}`;
}

function endpointTagArray(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    if (typeof item === 'string') return [item];
    const object = objectValue(item);
    const tag = object ? text(object, ['tag']) : undefined;
    return tag ? [tag] : [];
  });
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === 'string')
    : [];
}

function objectValue(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

function text(object: Record<string, unknown>, keys: string[]): string | undefined {
  for (const key of keys) {
    const value = object[key];
    if (typeof value === 'string' && value.trim()) return value;
    if (typeof value === 'number' && Number.isFinite(value)) return String(value);
  }
  return undefined;
}

function number(object: Record<string, unknown>, keys: string[]): number | undefined {
  for (const key of keys) {
    const value = object[key];
    if (typeof value === 'number' && Number.isFinite(value)) return value;
  }
  return undefined;
}
