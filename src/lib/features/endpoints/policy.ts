import { kernelFeatureSupport } from '$lib/services/kernel-capabilities';
import type { EndpointAction, EndpointCatalog, NetworkEndpoint } from './types';

function compatibleControl(catalog: EndpointCatalog | null): boolean {
  const contract = catalog?.capabilities.contracts?.controlApi;
  return !!contract && contract.minimumSupported === 1 && contract.current >= 1;
}
export function catalogSupported(catalog: EndpointCatalog | null): boolean {
  return compatibleControl(catalog) && kernelFeatureSupport(catalog?.capabilities, 'network_endpoint_catalog_v1').state === 'supported';
}
export function operationReason(catalog: EndpointCatalog | null, endpoint: NetworkEndpoint, action: EndpointAction): string | null {
  if (!compatibleControl(catalog) || kernelFeatureSupport(catalog?.capabilities, 'network_endpoint_control_v1').state !== 'supported') return '当前内核未声明端点控制能力';
  if (!catalog?.profileId || !catalog.editableEndpointIds.includes(endpoint.endpoint_id)) return '此端点没有关联到当前配置的端点声明，仅可观测';
  if (!Number.isSafeInteger(endpoint.config_revision)) return '配置版本超过客户端可安全处理的范围';
  if (!endpoint.supported.operations.includes(action.operation)) return '此端点未注册该操作';
  if (!Number.isSafeInteger(endpoint.intent_revision)) return '端点版本超过客户端可安全处理的范围';
  if (endpoint.state === 'starting' || endpoint.state === 'stopping') return '端点正在协调，请等待完成';
  if (action.operation === 'restart' && !endpoint.enabled) return '请先启用端点';
  if (action.operation === 'set_directions') {
    const requested = action.directions;
    if ((requested.inbound && !endpoint.supported.directions.inbound) || (requested.outbound && !endpoint.supported.directions.outbound)) return '此方向没有配置执行角色';
    if (directionRequiresRestart(catalog, endpoint, requested) &&
      (kernelFeatureSupport(catalog?.capabilities, 'network_endpoint_control_preconditions_v1').state !== 'supported' || !endpoint.supported.operations.includes('set_state'))) return '当前内核缺少安全切换方向的控制能力，请升级内核';
  }
  return null;
}
export function directionRequiresRestart(catalog: EndpointCatalog | null, endpoint: NetworkEndpoint, requested: NetworkEndpoint['allowed']): boolean {
  if (endpoint.state !== 'running') return false;
  const shrinkInbound = endpoint.allowed.inbound && !requested.inbound;
  const shrinkOutbound = endpoint.allowed.outbound && !requested.outbound;
  const live = kernelFeatureSupport(catalog?.capabilities, 'network_endpoint_operation_capabilities_v1').state === 'supported'
    ? endpoint.supported.operation_capabilities?.set_directions?.live_direction_contraction : undefined;
  if (live) return (shrinkInbound && !live.inbound) || (shrinkOutbound && !live.outbound);
  const limits = catalog?.capabilities.globalLimitations ?? [];
  return (limits.includes('endpoint_live_direction_contraction_requires_stop') && (shrinkInbound || shrinkOutbound)) ||
    (limits.includes('endpoint_live_outbound_direction_contraction_requires_stop') && shrinkOutbound);
}
export function endpointStateLabel(endpoint: NetworkEndpoint): string {
  return { stopped: '已停止', starting: '启动中', running: '运行中', stopping: '停止中', failed: '失败' }[endpoint.state] ?? endpoint.state;
}
export function directionLabel(directions: NetworkEndpoint['allowed']): string {
  return directions.inbound && directions.outbound ? '入站 + 出站' : directions.inbound ? '仅入站' : directions.outbound ? '仅出站' : '均不允许';
}
