import type { TrafficSnapshot, TrafficPage, TrafficScope, ResetSnapshot, U64 } from '$lib/features/traffic/types';
export function u64(value:unknown):U64 {
  const text=typeof value==='string'?value:typeof value==='number'&&Number.isSafeInteger(value)?String(value):'';
  if (!/^(0|[1-9]\d*)$/.test(text) || BigInt(text)>18446744073709551615n) throw new Error('统计字段不是无损 u64');
  return text;
}
function object(value:unknown):Record<string,unknown> { if(!value||typeof value!=='object'||Array.isArray(value))throw new Error('无效统计快照');return value as Record<string,unknown>; }
function text(value:unknown):string { if(typeof value!=='string'||!value.length)throw new Error('统计身份缺失');return value; }
function strings(value:unknown):string[]{ if(!Array.isArray(value)||value.some(v=>typeof v!=='string'))throw new Error('无效统计指标声明');return value; }
export function scope(value:unknown):TrafficScope {
  const v=object(value);
  if(v.kind==='global')return {kind:'global'};
  if(v.kind==='inbound'||v.kind==='outbound')return {kind:v.kind,tag:text(v.tag)};
  if(v.kind==='endpoint')return {kind:v.kind,endpoint_id:text(v.endpoint_id)};
  if(v.kind==='peer')return {kind:v.kind,endpoint_id:text(v.endpoint_id),peer_id:text(v.peer_id)};
  throw new Error('未知统计范围');
}
function counters(value:unknown):Record<string,U64|null> { return Object.fromEntries(Object.entries(object(value)).map(([k,v])=>[k,v==null?null:u64(v)])); }
export function snapshot(value:unknown):TrafficSnapshot {
  const v=object(value);
  if(!Array.isArray(v.planes))throw new Error('统计平面缺失');
  const planes=v.planes.map(raw=>{const p=object(raw);return {plane:text(p.plane),accounting_basis:text(p.accounting_basis),source_roles:p.source_roles==null?[]:strings(p.source_roles),counters:counters(p.counters),available_metrics:strings(p.available_metrics),resettable_metrics:strings(p.resettable_metrics)};});
  if(new Set(planes.map(p=>p.plane)).size!==planes.length)throw new Error('重复统计平面');
  return {scope:scope(v.scope),core_instance_id:text(v.core_instance_id),config_revision:u64(v.config_revision),generation:v.generation==null?null:u64(v.generation),stats_epoch:text(v.stats_epoch),epoch_started_at_unix_ms:u64(v.epoch_started_at_unix_ms),capture_started_at_unix_ms:u64(v.capture_started_at_unix_ms),sampled_at_monotonic_ns:u64(v.sampled_at_monotonic_ns),sampled_at_unix_ms:u64(v.sampled_at_unix_ms),planes,activity:counters(v.activity),reset_policy:text(v.reset_policy)};
}
function size(v:unknown):number {if(typeof v!=='number'||!Number.isSafeInteger(v)||v<0)throw new Error('无效分页游标');return v;}
export function page(value:unknown):TrafficPage {
  const v=object(value);if(!Array.isArray(v.scopes))throw new Error('统计页缺失');
  const result={core_instance_id:text(v.core_instance_id),config_revision:u64(v.config_revision),registry_revision:u64(v.registry_revision),sampled_at_unix_ms:u64(v.sampled_at_unix_ms),scopes:v.scopes.map(snapshot),total:size(v.total),next_offset:v.next_offset==null?null:size(v.next_offset)};
  if(result.scopes.some(s=>s.core_instance_id!==result.core_instance_id||s.config_revision!==result.config_revision))throw new Error('统计页混用了实例或配置版本');
  return result;
}
export function reset(value:unknown):ResetSnapshot {
  const v=object(value);if(!Array.isArray(v.snapshots))throw new Error('清空确认缺失');
  const result={core_instance_id:text(v.core_instance_id),operation_id:text(v.operation_id),snapshots:v.snapshots.map(snapshot)};
  if(result.snapshots.some(s=>s.core_instance_id!==result.core_instance_id))throw new Error('清空确认实例不一致');
  return result;
}
