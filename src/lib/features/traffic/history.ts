import type { TrafficSnapshot, PlaneSnapshot } from '$lib/features/traffic/types';
export const WINDOW_MS=120_000;
export const MAX_POINTS=121;
export interface Point { at:number; values:Record<string,number|null> }
export interface Observation { snapshot:TrafficSnapshot; rates:Record<string,number|null>; points:Point[]; seenAt:number; baselineValid:boolean }
export function metric(plane:PlaneSnapshot|undefined,key:string):bigint|null {
  const value=plane?.counters[key];return plane?.available_metrics.includes(key)&&value!=null?BigInt(value):null;
}
export function observe(previous:Observation|undefined,snapshot:TrafficSnapshot,retainHistory:boolean,now=Date.now()):Observation {
  const same=previous?.snapshot.core_instance_id===snapshot.core_instance_id&&previous.snapshot.stats_epoch===snapshot.stats_epoch&&previous.snapshot.generation===snapshot.generation;
  const before=same&&previous?.baselineValid?previous.snapshot:undefined;
  const elapsed=before?BigInt(snapshot.sampled_at_monotonic_ns)-BigInt(before.sampled_at_monotonic_ns):0n;
  const rates:Record<string,number|null>={};
  for(const plane of snapshot.planes)for(const key of new Set([...plane.available_metrics,'bytes_up','bytes_down','rx_bytes','tx_bytes'])) {
    if(!key.endsWith('bytes')&&!key.startsWith('bytes_'))continue;
    const current=metric(plane,key);const old=metric(before?.planes.find(p=>p.plane===plane.plane),key);
    rates[`${plane.plane}.${key}`]=elapsed>0n&&current!==null&&old!==null&&current>=old?Number((current-old)*1_000_000_000n*1_000_000n/elapsed)/1_000_000:null;
  }
  const at=Number(BigInt(snapshot.sampled_at_unix_ms));
  // Wall clock is only a chart coordinate. Monotonic time owns the rates.
  const points=retainHistory&&Number.isSafeInteger(at)?[...(same?previous?.points??[]:[]),{at,values:rates}].filter(p=>p.at<=at&&at-p.at<=WINDOW_MS).slice(-MAX_POINTS):[];
  return {snapshot,rates,points,seenAt:now,baselineValid:true};
}
export function rebaseline(row:Observation):Observation {return {...row,rates:{},baselineValid:false};}
export function byteKeys(snapshot:TrafficSnapshot,plane:string):[string,string] {return plane==='flow'&&snapshot.scope.kind!=='inbound'&&snapshot.scope.kind!=='outbound'?['bytes_down','bytes_up']:['rx_bytes','tx_bytes'];}
export function formatBytes(value:bigint|null):string {
  if(value===null)return '—';if(value<1024n)return `${value} B`;
  const units=['KB','MB','GB','TB','PB','EB'];let scale=1024n,index=0;
  while(value>=scale*1024n&&index<units.length-1){scale*=1024n;index++;}
  const rounded=(value*10n+scale/2n)/scale;return `${rounded/10n}.${rounded%10n} ${units[index]}`;
}
export function formatRate(value:number|null|undefined):string {if(value==null||!Number.isFinite(value))return '—';if(value<1024)return `${Math.round(value)} B/s`;if(value<1048576)return `${(value/1024).toFixed(1)} KB/s`;return `${(value/1048576).toFixed(1)} MB/s`;}
export function chartPath(points:Point[],metricKey:string,ceiling:number):string {
  const end=points.at(-1)?.at??0;let open=false;
  return points.map(p=>{const value=p.values[metricKey];if(value==null){open=false;return '';}
    const x=Math.max(0,300-(end-p.at)/WINDOW_MS*300),y=57-Math.min(1,value/ceiling)*48;
    const path=`${open?'L':'M'}${x.toFixed(2)},${y.toFixed(2)}`;open=true;return path;}).join(' ');
}
