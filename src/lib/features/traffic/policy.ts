import { kernelFeatureSupport } from '$lib/services/kernel-capabilities';
import type { TrafficDiscovery, TrafficSnapshot } from '$lib/features/traffic/types';
export function supported(discovery:TrafficDiscovery|null):boolean {
  const cap=discovery?.capabilities.trafficStatistics;
  const contract=discovery?.capabilities.contracts?.controlApi;
  return !!discovery?.supported && kernelFeatureSupport(discovery.capabilities,'traffic_observation_v1').state==='supported'
    && contract?.minimumSupported===1 && contract.current>=1 && cap?.contract_version===1 && Array.isArray(cap.queries) && cap.queries.includes('traffic_stats');
}
export function samplingSupported(d:TrafficDiscovery|null):boolean {
  const cap=d?.capabilities.trafficStatistics;
  return supported(d) && kernelFeatureSupport(d?.capabilities,'traffic_scopes_sampling_v1').state==='supported'
    && !!cap?.automatic_sampling && Number.isSafeInteger(cap.sample_page_size) && cap.sample_page_size > 0 && Number.isSafeInteger(cap.sample_interval_ms) && cap.sample_interval_ms > 0 && Array.isArray(cap.events) && cap.events.includes('stats.scopes_sampled') && cap.events.includes('stats.reset');
}
export function resetReason(d:TrafficDiscovery|null, row:TrafficSnapshot):string|null {
  const cap=d?.capabilities.trafficStatistics;
  if(!supported(d)||kernelFeatureSupport(d?.capabilities,'traffic_period_reset_v1').state!=='supported'||cap?.reset_command!=='stats.reset'||cap.reset_permission!=='admin'||cap.reset_policy!=='all_available_cumulative_no_cascade')return '当前内核不支持统计清空';
  if(!d?.admin)return '需要管理员权限';
  if(!Array.isArray(cap.resettable_scopes)||!cap.resettable_scopes.includes(row.scope.kind)||row.reset_policy!==cap.reset_policy)return '此范围不能清空';
  if(!row.planes.some(p=>p.resettable_metrics.some(m=>p.available_metrics.includes(m)&&p.counters[m]!=null)))return '没有可清空的累计指标';
  return null;
}
export function errorCode(error:unknown):string|null {
  if(!error||typeof error!=='object')return null;
  const e=error as {code?:string;details?:{error?:{code?:string}}};return e.details?.error?.code??e.code??null;
}
