import type { NetworkEndpoint } from './types';
import type { Observation } from '$lib/features/traffic/history';
import { scopeKey, type TrafficScope } from '$lib/features/traffic/types';

/** Legacy role statistics are shown as that role, never as endpoint totals. */
export function endpointObservation(endpoint: NetworkEndpoint, rows: Record<string, Observation>) {
  const endpointScope: TrafficScope = { kind: 'endpoint', endpoint_id: endpoint.endpoint_id };
  const endpointKey = scopeKey(endpointScope);
  let scope: TrafficScope = endpointScope;
  if (!rows[endpointKey] && endpoint.configuration?.origin === 'legacy') {
    if (endpoint.inbound_tags.length === 0 && endpoint.outbound_tags.length === 1) {
      scope = { kind: 'outbound', tag: endpoint.outbound_tags[0] };
    } else if (endpoint.outbound_tags.length === 0 && endpoint.inbound_tags.length === 1) {
      scope = { kind: 'inbound', tag: endpoint.inbound_tags[0] };
    }
  }
  const key = scopeKey(scope), row = rows[key];
  const valid = row && row.snapshot.core_instance_id === endpoint.core_instance_id
    && row.snapshot.config_revision === String(endpoint.config_revision)
    && scopeKey(row.snapshot.scope) === key
    && (scope.kind !== 'endpoint' || row.snapshot.generation === (endpoint.generation === null ? null : String(endpoint.generation)));
  return { key, observation: valid ? row : undefined };
}
