import type { GuiZeroCapabilities } from '$lib/types/gui-api';
// All u64 values cross Tauri as decimal strings, including clocks/revisions.
export type U64 = string;
export type TrafficScope = { kind:'global' } | { kind:'inbound'; tag:string } | { kind:'outbound'; tag:string } | { kind:'endpoint'; endpoint_id:string } | { kind:'peer'; endpoint_id:string; peer_id:string };
export type Plane = 'flow'|'inner'|'outer'| (string & {});
export interface PlaneSnapshot { plane:Plane; accounting_basis:string; source_roles:string[]; counters:Record<string,U64|null>; available_metrics:string[]; resettable_metrics:string[] }
export interface TrafficSnapshot {
  scope:TrafficScope; core_instance_id:string; config_revision:U64; generation:U64|null;
  stats_epoch:string; epoch_started_at_unix_ms:U64; capture_started_at_unix_ms:U64;
  sampled_at_monotonic_ns:U64; sampled_at_unix_ms:U64;
  planes:PlaneSnapshot[]; activity:Record<string,U64|null>; reset_policy:string;
}
export interface TrafficPage { core_instance_id:string; config_revision:U64; registry_revision:U64; sampled_at_unix_ms:U64; scopes:TrafficSnapshot[]; total:number; next_offset:number|null }
export interface TrafficQuery { offset:number; limit:number; scopes?:TrafficScope[]; expected_core_instance_id?:string; expected_config_revision?:U64; expected_registry_revision?:U64 }
export interface ResetTarget { scope:TrafficScope; expected_stats_epoch:string; expected_generation?:U64 }
export interface ResetInput { expected_core_instance_id:string; operation_id:string; targets:ResetTarget[] }
export interface ResetSnapshot { operation_id:string; core_instance_id:string; snapshots:TrafficSnapshot[] }
export interface TrafficCapability {
  contract_version:number; queries:string[]; reset_command:string; reset_permission:string; resettable_scopes:string[];
  reset_policy:string; events:string[]; automatic_sampling:boolean; sample_interval_ms:number;
  sample_page_size:number; maximum_page_size:number; maximum_reset_targets:number;
  history:string; retry_policy:string; capture_consistency:string; sampling_strategy:string;
}
export interface TrafficDiscovery { capabilities:GuiZeroCapabilities; supported:boolean; transport:'ipc'; admin:boolean }
export interface TrafficEvent { type:'sample'|'reset'; instance:string; payload:TrafficPage|ResetSnapshot }
export type StreamStatus = 'subscribed'|'offline'|'gap'|'reconnecting'|'error';
export interface TrafficGateway {
  discover():Promise<TrafficDiscovery>;
  page(query:TrafficQuery):Promise<TrafficPage>;
  get(scope:TrafficScope):Promise<TrafficSnapshot>;
  reset(input:ResetInput):Promise<ResetSnapshot>;
  subscribe(event:(event:TrafficEvent)=>void,status:(state:StreamStatus)=>void):Promise<()=>void>;
}
export function scopeKey(scope:TrafficScope):string {
  return JSON.stringify(scope.kind==='global' ? ['global'] : scope.kind==='inbound'||scope.kind==='outbound' ? [scope.kind,scope.tag] : scope.kind==='endpoint' ? [scope.kind,scope.endpoint_id] : [scope.kind,scope.endpoint_id,scope.peer_id]);
}
export function scopeLabel(scope:TrafficScope):string { return scope.kind==='global'?'全局':scope.kind==='inbound'||scope.kind==='outbound'?scope.tag:scope.kind==='endpoint'?scope.endpoint_id:scope.peer_id; }
