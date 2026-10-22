import type { MobileLocationBinding } from '../../src/lib/features/mobile/location';
import type { ClientCoreSnapshot, NodeScreenSnapshot } from '../../src/lib/types/gui-api';
import type { ProxyConfigProfile } from '../../src/lib/types/domain';

// Only host facts are replaced. All navigation, guards and target panels are real.
const params = new URLSearchParams(location.search);
const offline = params.has('offline');
const enabled = () => params.get('fixture') === 'mobile-location';
const waitingLocation: (() => void)[] = [], waitingNodes: (() => void)[] = [];
export const mobileLocationFixture = {
  profileId: 'main', revision: '1', instance: '1', generation: '1', host: 'host-a',
  noProfile: false, missingNode: false, failRead: false,
  holdLocation: false, holdNodes: false,
  calls: [] as { kind: string; binding: string }[],
  releaseLocation() { this.holdLocation = false; waitingLocation.splice(0).forEach(resolve => resolve()); },
  releaseNodes() { this.holdNodes = false; waitingNodes.splice(0).forEach(resolve => resolve()); },
  replaceHost() { this.host = this.host === 'host-a' ? 'host-b' : 'host-a'; this.generation = String(BigInt(this.generation) + 1n); },
};
if (enabled()) Object.assign(window, { mobileLocationFixture });
function binding(): MobileLocationBinding {
  return { binding: `${mobileLocationFixture.host}:${mobileLocationFixture.profileId}:${mobileLocationFixture.revision}:${mobileLocationFixture.generation}`,
    profileId: mobileLocationFixture.profileId, configRevision: mobileLocationFixture.revision,
    coreInstanceId: mobileLocationFixture.instance, scopeGeneration: mobileLocationFixture.generation };
}
export async function getMobileLocationSnapshot() {
  if (!enabled()) return { binding: { binding:'overview-host-main-1',profileId:'main',configRevision:'1',coreInstanceId:'1',scopeGeneration:'1' }, profile:{id:'main',name:'日常网络配置'},reason:null };
  const current = binding();
  mobileLocationFixture.calls.push({kind:'location',binding:current.binding});
  const result = mobileLocationFixture.noProfile ? {binding:null,profile:null,reason:'尚未选择配置，请浏览本地列表或添加配置。'}
    : {binding:current,profile:{id:current.profileId,name:'同名配置'},reason:null};
  if (mobileLocationFixture.holdLocation) await new Promise<void>(resolve => waitingLocation.push(resolve));
  if (mobileLocationFixture.failRead) throw {code:'location_unavailable',message:'宿主定位资料读取失败，请重新核对。'};
  return result;
}
export function mobileLocationProfiles(): ProxyConfigProfile[] {
  return (mobileLocationFixture.noProfile ? ['work'] : ['main','work']).map(id=>({
    id,name:'同名配置',kernel:'zero',format:'json',active:!mobileLocationFixture.noProfile && id===mobileLocationFixture.profileId,
    updatedAtUnixMs:1,content:{},capabilities:{hasProxyNodes:true,hasProxyGroups:true,hasRouteRules:false,hasRuleSets:false,hasSelector:true,hasUrlTest:true,featureKeys:[]},
  }));
}
export function mobileLocationClientSnapshot(): ClientCoreSnapshot {
  if (mobileLocationFixture.failRead) throw {code:'location_unavailable',message:'宿主定位资料读取失败，请重新核对。'};
  return {revision:1,scopeGeneration:Number(mobileLocationFixture.generation),sourceStatus:offline?'offline':'ready',activeProbeJobs:[],
    scope:{profileId:mobileLocationFixture.profileId,configRevision:mobileLocationFixture.revision,coreInstanceId:Number(mobileLocationFixture.instance)}};
}
export async function mobileLocationNodes(): Promise<NodeScreenSnapshot> {
  const current=binding();mobileLocationFixture.calls.push({kind:'nodes',binding:current.binding});
  const scope={profileId:current.profileId,configRevision:current.configRevision,coreInstanceId:Number(current.coreInstanceId)};
  const groups=[
    {tag:'outer-auto',kind:'urltest',selected:'inner-auto',memberTags:['inner-auto','node-b']},
    {tag:'inner-auto',kind:'fallback',selected:'node-a',memberTags:['node-a','node-b']},
    {tag:'manual',kind:'selector',selected:'node-b',memberTags:['node-a','node-b']},
  ].map(group=>({...group,id:{profileId:current.profileId,configRevision:current.configRevision,tag:group.tag},runtimeAvailable:!offline,available:!offline}));
  const result:NodeScreenSnapshot={revision:1,scope,sourceStatus:offline?'offline':'ready',activeProbeJobs:[],groups,
    nodes:(mobileLocationFixture.missingNode?['node-b','independent']:['node-a','node-b','independent']).map(tag=>({
      id:{profileId:current.profileId,configRevision:current.configRevision,tag},tag,protocol:'vless',
      server:`${tag}.example.test`,port:443,groupTags:groups.filter(group=>group.memberTags.includes(tag)).map(group=>group.tag),
      selectedIn:groups.filter(group=>group.selected===tag).map(group=>group.tag),runtimeAvailable:!offline,actionValid:!offline,
      alive:tag!=='node-a' && !(params.get('target')==='independent' && tag==='independent'),
      latencyMs:tag==='node-a' || (params.get('target')==='independent' && tag==='independent')?undefined:42,
      lastObservedAtUnixMs:params.get('target')==='independent' && tag==='independent'?Date.now():undefined,
      activeProbeJobIds:[],history:[],
    }))};
  if(mobileLocationFixture.holdNodes)await new Promise<void>(resolve=>waitingNodes.push(resolve));
  if(mobileLocationFixture.failRead)throw {code:'location_unavailable',message:'节点目录读取失败，请重新核对。'};
  return result;
}
