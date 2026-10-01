import type { TrafficGateway, TrafficDiscovery, TrafficSnapshot, TrafficQuery, TrafficEvent, StreamStatus, ResetInput, TrafficScope } from '$lib/features/traffic/types';
import { scopeKey } from '$lib/features/traffic/types';
export function discovery():TrafficDiscovery {
  return {supported:true,admin:true,transport:'ipc',capabilities:{available:true,errorCodes:[],adapters:[],sinks:[],protocols:[],permissions:['read'],features:['traffic_observation_v1','traffic_period_reset_v1','traffic_scopes_sampling_v1'],buildFeatures:[],globalLimitations:[],contracts:{capabilities:{current:1,minimumSupported:1},controlApi:{current:1,minimumSupported:1},configSchema:{current:1,minimumSupported:1},errorCodes:{current:1,minimumSupported:1}},trafficStatistics:{contract_version:1,queries:['traffic_stat','traffic_stats'],reset_command:'stats.reset',reset_permission:'admin',resettable_scopes:['global','inbound','outbound','endpoint','peer'],reset_policy:'all_available_cumulative_no_cascade',events:['stats.scopes_sampled','stats.reset'],automatic_sampling:true,sample_interval_ms:1000,sample_page_size:64,maximum_page_size:64,maximum_reset_targets:256,history:'client_bounded',retry_policy:'query_after_unknown',capture_consistency:'scope_atomic',sampling_strategy:'round_robin'}}};
}
export function sample(scope:TrafficScope={kind:'global'}, index=0):TrafficSnapshot {
  const role=scope.kind==='inbound'||scope.kind==='outbound';
  const flow:Record<string,string|null>=role?{rx_bytes:'11000',tx_bytes:'2000'}:{bytes_down:'11000',bytes_up:'2000'};
  return {scope,core_instance_id:'core-a',config_revision:'9007199254740993',generation:scope.kind==='endpoint'||scope.kind==='peer'?'9007199254740994':null,stats_epoch:`epoch-${index}`,epoch_started_at_unix_ms:'1790899200000',capture_started_at_unix_ms:'1790899200000',sampled_at_monotonic_ns:'9007199254740993000',sampled_at_unix_ms:'1790899200000',reset_policy:'all_available_cumulative_no_cascade',planes:[
    {plane:'flow',accounting_basis:role?'execution_role_rx_tx':'logical_flow_bytes',source_roles:role?[scope.kind]:[],available_metrics:Object.keys(flow),resettable_metrics:Object.keys(flow),counters:{...flow,rx_packets:null,errors:null}},
    {plane:'inner',accounting_basis:'device_inner_ip',source_roles:['inbound','outbound'],available_metrics:['rx_bytes','tx_bytes','rx_packets','tx_packets'],resettable_metrics:['rx_bytes','tx_bytes','rx_packets','tx_packets'],counters:{rx_bytes:'10000',tx_bytes:'3000',rx_packets:'10',tx_packets:'2',errors:null}},
    {plane:'outer',accounting_basis:'protocol_carrier',source_roles:['inbound','outbound'],available_metrics:scope.kind==='endpoint'||scope.kind==='peer'?['rx_bytes','tx_bytes']:[],resettable_metrics:scope.kind==='endpoint'||scope.kind==='peer'?['rx_bytes','tx_bytes']:[],counters:{rx_bytes:scope.kind==='endpoint'||scope.kind==='peer'?'12000':null,tx_bytes:scope.kind==='endpoint'||scope.kind==='peer'?'4000':null,errors:null}}],activity:{active_stream_flows:'2',active_datagram_flows:'3',active_packet_routes:'1'}};
}
export const scopes:TrafficScope[]=[{kind:'global'},{kind:'inbound',tag:'mixed'},{kind:'outbound',tag:'proxy-a'},{kind:'endpoint',endpoint_id:'opaque:/wg-a'},{kind:'peer',endpoint_id:'opaque:/wg-a',peer_id:'opaque:peer/one'}];
export class FixtureGateway implements TrafficGateway {
  caps=discovery(); rows=scopes.map(sample); registry='1';
  queries:TrafficQuery[]=[]; commands:ResetInput[]=[]; discoveries=0; stops=0;
  mode='success'; pageLimit=64; conflictOnce=false;
  event?: (event:TrafficEvent)=>void; status?: (status:StreamStatus)=>void;
  async discover(){this.discoveries++;return structuredClone(this.caps);}
  async page(query:TrafficQuery){
    this.queries.push(structuredClone(query));
    if(this.conflictOnce&&query.offset){this.conflictOnce=false;this.registry='2';throw {code:'conflict'};}
    if(query.expected_registry_revision&&query.expected_registry_revision!==this.registry)throw {code:'conflict'};
    const count=Math.min(query.limit,this.pageLimit),rows=this.rows.slice(query.offset,query.offset+count);
    return {core_instance_id:this.rows[0]?.core_instance_id??'core-a',config_revision:this.rows[0]?.config_revision??'9007199254740993',registry_revision:this.registry,sampled_at_unix_ms:this.rows[0]?.sampled_at_unix_ms??'1790899200000',scopes:structuredClone(rows),total:this.rows.length,next_offset:query.offset+count<this.rows.length?query.offset+count:null};
  }
  async get(scope:TrafficScope){const row=this.rows.find(s=>scopeKey(s.scope)===scopeKey(scope));if(!row)throw{code:'not_found'};return structuredClone(row);}
  async reset(input:ResetInput){
    this.commands.push(structuredClone(input));
    if(this.mode!=='success'&&this.mode!=='lost_ack')throw {code:this.mode,message:this.mode};
    if(input.expected_core_instance_id!==this.rows[0]?.core_instance_id)throw{code:'conflict'};
    const affected=input.targets.map(t=>{
      const row=this.rows.find(r=>scopeKey(r.scope)===scopeKey(t.scope));
      if(!row)throw{code:'not_found'};
      if(row.stats_epoch!==t.expected_stats_epoch||(t.expected_generation!==undefined&&row.generation!==t.expected_generation))throw{code:'conflict'};
      return row;
    });
    for(const row of affected){
      row.stats_epoch=`reset-${this.commands.length}-${scopeKey(row.scope)}`;
      row.epoch_started_at_unix_ms='1790899200500';
      row.sampled_at_monotonic_ns=String(BigInt(row.sampled_at_monotonic_ns)+500_000_000n);
      for(const p of row.planes)for(const m of p.resettable_metrics)p.counters[m]=m.endsWith('bytes')||m.startsWith('bytes_')?'5':'0';
    }
    if(this.mode==='lost_ack')throw{code:'connection_closed'};
    return {operation_id:input.operation_id,core_instance_id:input.expected_core_instance_id,snapshots:structuredClone(affected)};
  }
  async subscribe(event:(event:TrafficEvent)=>void,status:(state:StreamStatus)=>void){this.event=event;this.status=status;status(this.caps.capabilities.trafficStatistics?.automatic_sampling?'subscribed':'offline');return()=>{this.stops++;this.event=undefined;this.status=undefined;};}
  advance(indices=this.rows.map((_,i)=>i),delta=1000){
    for(const index of indices){const row=this.rows[index];row.sampled_at_monotonic_ns=String(BigInt(row.sampled_at_monotonic_ns)+BigInt(delta)*1_000_000n);row.sampled_at_unix_ms=String(BigInt(row.sampled_at_unix_ms)+BigInt(delta));for(const p of row.planes)for(const k of p.available_metrics)if(p.counters[k]!=null)p.counters[k]=String(BigInt(p.counters[k]!)+1000n);}
  }
  push(indices=this.rows.map((_,i)=>i)){
    this.event?.({type:'sample',instance:this.rows[0].core_instance_id,payload:{core_instance_id:this.rows[0].core_instance_id,config_revision:this.rows[0].config_revision,registry_revision:this.registry,sampled_at_unix_ms:this.rows[0].sampled_at_unix_ms,scopes:structuredClone(indices.map(i=>this.rows[i])),total:this.rows.length,next_offset:null}});
  }
}
