import type { ClientCoreSnapshot } from '$lib/types/gui-api';
import type { NetworkEndpoint } from './types';
import type { TrafficView } from '$lib/features/traffic/session';
import type { TrafficScope } from '$lib/features/traffic/types';
import { availablePlanes } from '$lib/features/traffic/presentation';
import { u64 } from '$lib/features/traffic/wire';
import { supported } from '$lib/features/traffic/policy';

/** Fixed source identity. Labels never supply a registry identity. */
export interface EndpointTrafficTarget {
  profileId: string | null; endpointId: string; protocol: string; label: string;
  coreInstanceId: string; configRevision: string; generation: string | null;
  legacyRole: Extract<TrafficScope, { kind: 'inbound' | 'outbound' }> | null;
}
/** UI return continuation; it grants neither statistics nor endpoint control. */
export interface EndpointTrafficVisit {
  target: EndpointTrafficTarget;
  isCurrent: () => boolean;
  verifyCurrent: () => Promise<boolean>;
  restore: () => void;
}
const safe = (value: number) => Number.isSafeInteger(value) && value >= 0;
/** Client-only generation is not a remote Zero revision or authority grant. */
export function endpointClientBinding(snapshot: ClientCoreSnapshot): string | undefined {
  const generation = snapshot.scopeGeneration;
  if (generation === undefined || !safe(generation) || generation === 0 || !safe(snapshot.scope.coreInstanceId)) return undefined;
  let revision: string;
  try { revision = u64(snapshot.scope.configRevision); } catch { return undefined; }
  return JSON.stringify([snapshot.scope.profileId ?? null, revision, snapshot.scope.coreInstanceId, generation]);
}
export function endpointTrafficTarget(profileId: string | null, endpoint: NetworkEndpoint): EndpointTrafficTarget | null {
  if (!endpoint.endpoint_id || !endpoint.core_instance_id || !safe(endpoint.config_revision)
    || (endpoint.generation !== null && !safe(endpoint.generation))) return null;
  let legacyRole: EndpointTrafficTarget['legacyRole'] = null;
  if (endpoint.configuration?.origin === 'legacy') {
    if (!endpoint.inbound_tags.length && endpoint.outbound_tags.length === 1) legacyRole = { kind: 'outbound', tag: endpoint.outbound_tags[0] };
    else if (!endpoint.outbound_tags.length && endpoint.inbound_tags.length === 1) legacyRole = { kind: 'inbound', tag: endpoint.inbound_tags[0] };
  }
  return { profileId, endpointId: endpoint.endpoint_id, protocol: endpoint.protocol, label: endpoint.tag,
    coreInstanceId: endpoint.core_instance_id, configRevision: String(endpoint.config_revision),
    generation: endpoint.generation === null ? null : String(endpoint.generation), legacyRole };
}

export function endpointTrafficUnavailableReason(endpoint: NetworkEndpoint, stale: boolean, clientBinding?: string): string | null {
  if (stale) return '端点目录未确认，请刷新后查看统计。';
  if (!clientBinding) return '客户端身份或运行代次未确认，请刷新；旧版宿主需升级后才能固定此端点。';
  if (!endpointTrafficTarget(null, endpoint)) return '端点身份、配置版本或 generation 超出安全范围，不能创建统计定位。';
  return null;
}
export function matchesEndpointTrafficTarget(target: EndpointTrafficTarget, profileId: string | null, endpoint: NetworkEndpoint): boolean {
  const current = endpointTrafficTarget(profileId, endpoint);
  return current !== null && JSON.stringify(current) === JSON.stringify(target);
}
/** Resolve only an actual inventory row, with no fabricated scope or global fallback. */
export function resolveEndpointTraffic(target: EndpointTrafficTarget, view: TrafficView, sourceCurrent = true) {
  const unavailable = (reason: string) => ({ key: null, planes: [] as string[], reason, role: false });
  if (!sourceCurrent) return unavailable('原端点、配置或客户端运行代次已变化或未确认，无法继续定位；请返回端点目录重新选择。');
  if (!view.discovery && view.loading) return unavailable('正在读取真实统计目录…');
  if (!supported(view.discovery)) return unavailable('当前内核未提供此统计能力，请重新检查或升级内核。');
  let key = view.order.find(key => { const scope = view.rows[key]?.snapshot.scope; return scope?.kind === 'endpoint' && scope.endpoint_id === target.endpointId; });
  if (!key && target.legacyRole) {
    const role = target.legacyRole;
    key = view.order.find(key => { const scope = view.rows[key]?.snapshot.scope; return scope?.kind === role.kind && 'tag' in scope && scope.tag === role.tag; });
  }
  if (!key) return unavailable(view.loading || view.stale ? '统计目录尚未确认，请重新检查。' : '统计 registry 未登记此端点的范围；没有生成替代统计。');
  const snapshot = view.rows[key].snapshot;
  if (snapshot.core_instance_id !== target.coreInstanceId || snapshot.config_revision !== target.configRevision
    || (snapshot.scope.kind === 'endpoint' && snapshot.generation !== target.generation)) {
    return unavailable('统计实例、配置版本或端点 generation 已变化，请返回端点目录重新选择。');
  }
  const role = snapshot.scope.kind !== 'endpoint';
  const planes = availablePlanes([snapshot]);
  const reason = view.stale ? '统计采样中断，累计值为上次观测；速率暂不可用。'
    : !planes.length ? '此实际范围没有声明可用统计平面。'
    : role ? `当前只提供实际${snapshot.scope.kind === 'inbound' ? '入站' : '出站'}角色统计，不代表端点总计。` : null;
  return { key, planes, reason, role };
}
