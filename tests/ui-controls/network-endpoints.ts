import type { EndpointCatalog, EndpointGateway, NetworkEndpoint } from '../../src/lib/features/endpoints/types';
import { capabilityFixture } from './kernel-capabilities';
let configuredPeer = 'example.test:51820';
window.addEventListener('fixture-change-peer', () => { configuredPeer = 'changed.test:51820'; });
const params = () => new URLSearchParams(location.search);
const base = (tag: string, protocol: string): NetworkEndpoint => ({
  endpoint_id:`resource:${tag}`,tag,protocol,inbound_tags:[`endpoint/${tag}`],outbound_tags:[tag],
  supported:{directions:{inbound:true,outbound:true},operations:['list','get','details','set_state','set_directions','restart','clear_overrides'],packet:true,stream:false,datagram:false,derived_stream:true,derived_datagram:true},
  enabled:true,allowed:{inbound:true,outbound:true},effective:{inbound:true,outbound:true},state:'running',health:'unknown',state_source:'config',
  core_instance_id:'core-1',config_revision:1,intent_revision:7,generation:1,observed_at_unix_ms:Date.now(),started_at_unix_ms:Date.now()-180000,
  counters:{inner_rx_bytes:null,inner_tx_bytes:null,outer_rx_bytes:null,outer_tx_bytes:null,active_packet_routes:null,active_stream_flows:0,active_datagram_flows:2},last_error:null,
});
let endpoints=[base('wg-a','wireguard'),base('mesh-b','future_mesh')];
if (params().has('legacy-role-stats')) endpoints=endpoints.map(row=>({...row,configuration:{origin:'legacy'},endpoint_id:`opaque-role/${row.tag}`,inbound_tags:[],allowed:{inbound:false,outbound:true},effective:{inbound:false,outbound:true},counters:{...row.counters,active_stream_flows:0,active_datagram_flows:0,active_packet_routes:null}}));
if (params().has('long-details')) {
  endpoints[0].inbound_tags = ['endpoint/' + 'long-resource-name-'.repeat(8)];
  endpoints[0].outbound_tags = ['outbound/' + 'long-resource-name-'.repeat(8)];
}
let lastSample = Date.now();
let sample = 0;
let frozen = false;
window.addEventListener('fixture-freeze-traffic', () => { frozen = true; });
window.addEventListener('fixture-resume-traffic', () => { frozen = false; });
function observeTraffic() {
  if (params().has('frozen-samples') && frozen) return;
  const now = Date.now();
  const elapsed = Math.max(0, now - lastSample) / 1000;
  sample++;
  endpoints.forEach((row, index) => {
    row.observed_at_unix_ms = now;
    if (params().has('missing-counters')) return;
    const activity = row.enabled ? .55 + .45 * Math.sin(sample * 1.3 + index) : 0;
    row.counters.inner_rx_bytes = (row.counters.inner_rx_bytes ?? 2500000) + Math.round(elapsed * activity * (index ? 16000 : 84000));
    row.counters.inner_tx_bytes = (row.counters.inner_tx_bytes ?? 1400000) + Math.round(elapsed * activity * (index ? 42000 : 24000));
    if (params().has('zero-counters')) { row.counters.inner_rx_bytes = 0; row.counters.inner_tx_bytes = 0; }
  });
  lastSample = now;
}
// This fixture models the client overlay, never a kernel source-file save.
const localOverrides: Record<string, {enabled?:boolean;directions?:NetworkEndpoint['allowed']}> = {};

window.addEventListener('fixture-reload-config', () => {
  endpoints.forEach(row => {
    const preference=localOverrides[row.endpoint_id];
    row.enabled=preference?.enabled ?? true;
    row.allowed={...(preference?.directions ?? {inbound:true,outbound:true})};
    row.state=row.enabled?'running':'stopped';
    row.effective=row.enabled?{...row.allowed}:{inbound:false,outbound:false};
    row.config_revision++;row.intent_revision++;row.generation=(row.generation??0)+1;
    row.state_source='config';
  });
});
export const endpointGateway: EndpointGateway = {
  async catalog(): Promise<EndpointCatalog> {
    window.dispatchEvent(new Event('fixture-endpoint-catalog-read'));
    if (params().has('slow-observation')) await new Promise(resolve => setTimeout(resolve, 250));
    observeTraffic();
    const capabilities=capabilityFixture();
    if (params().get('version')==='v002') return {capabilities,endpoints:[],profileId:'profile-a',editableEndpointIds:[],localOverrideIds:[]};
    capabilities.features=['network_endpoint_catalog_v1','network_endpoint_control_v1',...(params().has('old-preconditions')?[]:['network_endpoint_control_preconditions_v1'])];
    capabilities.globalLimitations=[params().has('old-control') ? 'endpoint_live_direction_contraction_requires_stop' : 'endpoint_live_outbound_direction_contraction_requires_stop'];
    if (params().has('observe-only')) endpoints.forEach(row => { row.supported.operations=['list','get','details']; });
    return {capabilities,endpoints:structuredClone(endpoints),profileId:'profile-a',editableEndpointIds:params().has('unowned')?[]:endpoints.map(row=>row.endpoint_id),localOverrideIds:Object.keys(localOverrides),configuredAddresses:{[endpoints[0].endpoint_id]:['10.66.0.2/32','fd66:1234:5678:90ab::2/128']}};
  },
  async control(input) {
    window.dispatchEvent(new CustomEvent('fixture-save',{detail:input}));
    if (params().has('conflict')) throw {code:'conflict',message:'端点意图已变化，请刷新后重试'};
    if (params().has('delayed')) await new Promise(resolve=>setTimeout(resolve,700));
    const row=endpoints.find(row=>row.endpoint_id===input.endpointId)!;
    if (input.profileId!=='profile-a' || input.configRevision!==row.config_revision || input.expectedIntentRevision!==row.intent_revision) throw {message:'配置已变化'};
    const action=input.action;
    const restartRequired=action.operation==='set_directions' && row.state==='running' &&
      ((row.allowed.outbound && !action.directions.outbound) || (params().has('old-control') && row.allowed.inbound && !action.directions.inbound));
    if(restartRequired && action.operation==='set_directions') {
      const original=structuredClone(row);
      row.enabled=false;row.state='stopped';row.intent_revision++;
      row.allowed={...action.directions};row.intent_revision++;
      if(params().has('restart-failure')) {
        row.allowed={...original.allowed};row.enabled=original.enabled;row.state=original.state;row.intent_revision+=2;
        throw {code:'internal',message:'方向切换失败，已恢复原状态：端点启用失败'};
      }
      row.enabled=original.enabled;row.state=original.state;row.intent_revision++;
    } else {
      if(action.operation==='set_state') row.enabled=action.enabled;
      if(action.operation==='set_directions') row.allowed={...action.directions};
      if(action.operation!=='restart') row.intent_revision++;
    }
    if(action.operation==='set_state') (localOverrides[row.endpoint_id] ??= {}).enabled=action.enabled;
    if(action.operation==='set_directions') (localOverrides[row.endpoint_id] ??= {}).directions={...action.directions};
    row.state=row.enabled?'running':'stopped';
    row.effective=row.enabled?{...row.allowed}:{inbound:false,outbound:false};
    row.generation=(row.generation ?? 0)+1;
    if(action.operation!=='restart') row.state_source='runtime_override';
    return structuredClone(row);
  },
  async details(endpointId) {
    if (params().has('slow-observation')) await new Promise(resolve => setTimeout(resolve, 250));
    const row=endpoints.find(row=>row.endpoint_id===endpointId)!;
    return {endpoint_id:endpointId,generation:row.generation,schema_id:row.protocol==='wireguard'?'zero.endpoint.wireguard.v1':'zero.endpoint.future.v1',schema_version:1,
      details:row.protocol==='wireguard'?{peers:[{peer_id:'peer-1',public_key:params().has('long-details') ? 'q3b2T3yN7WvK5sR8dM1xP4cL6jH9aF0uE2nG5zB7kVQ=' : 'public-only',allowed_ips:params().has('long-details') ? ['10.0.0.0/24','fd00:1234:5678:9abc::/64'] : ['10.0.0.0/24'],configured_endpoint:params().has('long-details') ? 'wireguard.' + 'long-hostname.'.repeat(6) + 'example.test:51820' : configuredPeer,authenticated_endpoint:null,source_known:null,health:null}]}:{connected:null}};
  },
};
