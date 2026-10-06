import { FixtureGateway, sample } from '../fixtures/traffic';
import type { TrafficGateway } from '$lib/features/traffic/types';
const params = new URLSearchParams(window.location.search);
const enabled = params.has('traffic-v1');
const fixture = new FixtureGateway();
if(params.has('traffic-many-scopes')) fixture.rows.push(...Array.from({length:26},(_,index)=>sample({kind:'outbound',tag:`extra-${index+1}`},index+5)));
fixture.rows[3].scope = {kind:'endpoint',endpoint_id:'resource:wg-a'};
fixture.rows[4].scope = {kind:'peer',endpoint_id:'resource:wg-a',peer_id:'peer-1'};
if(params.has('legacy-role-stats')) {
  fixture.rows[3]=sample({kind:'outbound',tag:'wg-a'},3);
  fixture.rows[4]=sample({kind:'outbound',tag:'mesh-b'},4);
  fixture.rows[3].planes[1].source_roles=['outbound'];
  fixture.rows[4].planes[1].available_metrics=[];fixture.rows[4].planes[1].resettable_metrics=[];
  for(const key of Object.keys(fixture.rows[4].planes[1].counters)) fixture.rows[4].planes[1].counters[key]=null;
  for(const row of fixture.rows.slice(3)) row.activity={active_stream_flows:'0',active_datagram_flows:'0',active_packet_routes:null};
}
for(const row of fixture.rows) {
  row.core_instance_id='core-1';row.config_revision='1';
  if(row.scope.kind==='endpoint'||row.scope.kind==='peer') row.generation='1';
}
if(params.has('missing-endpoint-stat')) fixture.rows=fixture.rows.filter(row=>row.scope.kind!=='endpoint');
fixture.pageLimit=2;
if (!enabled) { fixture.caps.supported=false; fixture.caps.capabilities.features=[]; }
if (params.has('traffic-poll')) fixture.caps.capabilities.trafficStatistics!.automatic_sampling=false;
fixture.mode=params.get('traffic-reset') ?? 'success';
let subscriptions=0;
function report() {
  let output=document.querySelector('output#traffic-fixture');
  if(!output){output=document.createElement('output');output.id='traffic-fixture';output.className='sr-only';document.body.append(output);}
  output.textContent=JSON.stringify({queries:fixture.queries.length,commands:fixture.commands,stops:fixture.stops,subscriptions});
}
export const trafficGateway:TrafficGateway={
  discover:()=>fixture.discover(),
  page:async input=>{const value=await fixture.page(input);report();return value;},
  get:scope=>fixture.get(scope),
  reset:async input=>{await new Promise(resolve=>setTimeout(resolve,200));try{return await fixture.reset(input);}finally{report();}},
  subscribe:async (event,status)=>{
    subscriptions++;
    const stop=await fixture.subscribe(event,status);
    report();
    const sample=()=>{fixture.advance();fixture.push();};
    const gap=()=>fixture.status?.('gap');
    const remove=()=>{fixture.rows=fixture.rows.filter(row=>row.scope.kind!=='peer');fixture.registry=String(BigInt(fixture.registry)+1n);fixture.status?.('gap');};
    window.addEventListener('fixture-traffic-sample',sample);
    window.addEventListener('fixture-traffic-gap',gap);
    window.addEventListener('fixture-traffic-remove-peer',remove);
    return()=>{window.removeEventListener('fixture-traffic-sample',sample);window.removeEventListener('fixture-traffic-gap',gap);window.removeEventListener('fixture-traffic-remove-peer',remove);stop();report();};
  },
};
