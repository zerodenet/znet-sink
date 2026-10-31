// Browser preview of the client-owned library, independent of kernel fixtures.
// Dedicated library tests inject desktop IPC and exercise the real gateway.
import * as desktop from '../../src/lib/features/endpoints/library';
import type { UserEndpoint, UserEndpointLibrary } from '../../src/lib/features/endpoints/library';
export type { UserEndpoint, UserEndpointLibrary } from '../../src/lib/features/endpoints/library';
let revision = 0;
let endpoints: UserEndpoint[] = new URLSearchParams(location.search).has('preview') ? [{
  tag: 'wg-home', enabled: true, directions: { inbound: false, outbound: true },
  protocol: { type: 'wireguard', private_key: 'AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=',
    addresses: ['10.66.0.2/32', 'fd66::2/128'], peers: [{
      public_key: 'AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=', endpoint: 'vpn.example.com:51820',
      allowed_ips: ['0.0.0.0/0', '::/0'],
    }] },
}] : [];
const snapshot = (): UserEndpointLibrary => structuredClone({ revision: String(revision), endpoints, protocols: [{ type: 'wireguard', label: 'WireGuard' }] });
export const loadUserEndpoints = () => '__TAURI_INTERNALS__' in window
  ? desktop.loadUserEndpoints() : Promise.resolve(snapshot());
export async function saveUserEndpoints(expectedRevision: string, rows: UserEndpoint[]) {
  if ('__TAURI_INTERNALS__' in window) return desktop.saveUserEndpoints(expectedRevision, rows);
  if (expectedRevision !== String(revision)) throw new Error('用户端点已变化，请刷新后重新编辑');
  if (new Set(rows.map(row => row.tag)).size !== rows.length) throw new Error('用户端点名称不能重复');
  endpoints = structuredClone(rows); revision++;
  window.dispatchEvent(new CustomEvent('fixture-user-endpoints-changed', { detail: rows.map(row => ({
    tag: row.tag, enabled: row.enabled, directions: row.directions, addresses: Array.isArray(row.protocol.addresses) ? row.protocol.addresses as string[] : [],
  })) }));
  return snapshot();
}
