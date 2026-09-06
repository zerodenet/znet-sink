import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile } from 'node:fs/promises';
import { SourceTextModule, SyntheticModule, createContext } from 'node:vm';
import ts from 'typescript';

// Exercise the real store methods with command/query adapters replaced at the
// import boundary. These tests do not launch a kernel or change the host network.
async function harness() {
  const state = {
    connection: { state:'connected',processState:'running',coreAvailable:true,systemProxyEnabled:true },
    mode: { currentMode:'rule',availableModes:['rule','global','direct'] },
    tun: { key:'tun',supported:true,enabled:true,desiredEnabled:true,healthy:true,addresses:[],ipv4Egress:{availability:'available'},ipv6Egress:{availability:'available'} },
    groups: [{name:'manual',kind:'selector',selected:'a',outbounds:[{tag:'a'},{tag:'b'},{tag:'auto',type:'urltest'}]}, {name:'auto',kind:'urltest',selected:'a',outbounds:[{tag:'a'}]}],
    reject: false, readFailure: false, connectionReadFailure: false, calls: [], notifications: [], wait: null,
  };
  const copy = value => structuredClone(value);
  const command = async (name, effect) => { state.calls.push(name); if (state.wait) await state.wait; if (state.reject) throw new Error('command rejected'); effect(); };
  const core = {
    getGuiSelfTestSnapshot: async()=>({ready:true,checks:[],blockingIssues:[],activeProxyConfigId:'main'}),
    getGuiConnectionStatus: async()=>{if(state.connectionReadFailure)throw new Error('status timeout');return copy(state.connection);},
    getGuiProxyModeStatus: async()=>copy(state.mode),
    guiSetProxyMode: async mode=>{await command('mode',()=>{state.mode.currentMode=mode;});return copy(state.mode);},
    guiSelectPolicy: async (group,target)=>{state.calls.push('policy');if(state.wait)await state.wait;if(state.reject)return {accepted:false,message:'not accepted'};state.groups.find(item=>item.name===group).selected=target;return {accepted:true};},
    getGuiCoreOverview: async()=>({coreState:'running'}),
    getGuiPolicyGroups: async()=>{if(state.readFailure)throw new Error('query failed');return copy(state.groups);},
    getConfigProxyNodes: async()=>[], getConfigPolicyGroups:async()=>copy(state.groups),
    getGuiZeroCapabilities:async()=>({available:true,features:['query']}),
    guiNetworkProbe:async()=>({ip:'192.0.2.1'}),trayUpdateStatus:async()=>{},
    getAppConfig:async()=>({tun:{enabled:state.tun.desiredEnabled}}),
    enableSystemProxy:async()=>command('proxy-on',()=>{state.connection.systemProxyEnabled=true;}),
    disableSystemProxy:async()=>command('proxy-off',()=>{state.connection.systemProxyEnabled=false;}),
    guiConnect:async()=>{},guiDisconnect:async()=>{},startCoreProcess:async()=>{},restartCoreProcess:async()=>{},
  };
  const tun = {
    getGuiTunStatus:async()=>copy(state.tun),
    enableGuiTun:async()=>{await command('tun-on',()=>{state.tun.enabled=state.tun.desiredEnabled=true;});return copy(state.tun);},
    disableGuiTun:async()=>{await command('tun-off',()=>{state.tun.enabled=state.tun.desiredEnabled=false;});return copy(state.tun);},
  };
  const context = createContext({ $state:value=>value,console:{warn(){}},Date,Set,Map,setTimeout,clearTimeout,setInterval,clearInterval });
  const adapters = { './core':core, './tun':tun, './toast.svelte':{error(message){state.notifications.push(['error',message]);},success(message){state.notifications.push(['success',message]);},warning(message){state.notifications.push(['warning',message]);}}, './telemetry':{tracedOperation:async(_area,_name,action)=>action()} };
  const cache=new Map();
  async function load(url) {
    if(cache.has(url))return cache.get(url);
    const source=await readFile(new URL(url),'utf8');
    const code=url.endsWith('.ts') ? ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext}}).outputText : source;
    const module=new SourceTextModule(code,{context,identifier:url});cache.set(url,module);
    await module.link(async(specifier,ref)=>{
      if(adapters[specifier]) {
        const exports=adapters[specifier];
        return new SyntheticModule(Object.keys(exports),function(){for(const [key,value] of Object.entries(exports))this.setExport(key,value);},{context});
      }
      return load(new URL(specifier.match(/\.(ts|js)$/)?specifier:`${specifier}.ts`,ref.identifier).href);
    });
    return module;
  }
  const module=await load(new URL('../src/lib/services/gui-state.svelte.ts',import.meta.url).href);
  await module.evaluate();
  const gui=module.namespace.guiState;gui.isInitializing=false;
  await gui.refreshAll();return {state,gui};
}

test('mode request is serialized and failures retain confirmed mode with a returned error',async()=>{
  const {gui,state}=await harness();let finish;state.wait=new Promise(resolve=>{finish=resolve;});
  const first=gui.setProxyMode('global');
  assert.equal(gui.proxyMode.currentMode,'rule');assert.equal(gui.isSwitchingMode,true);
  assert.equal((await gui.setProxyMode('direct')).ok,false);
  state.reject=true;finish();const result=await first;
  assert.equal(result.ok,false);assert.match(result.message,/rejected/);
  assert.equal(gui.proxyMode.currentMode,'rule');assert.equal(gui.isSwitchingMode,false);
  assert.deepEqual(state.calls,['mode']);
});
test('only selector direct members may be chosen, including nested group tags',async()=>{
  const {gui,state}=await harness();
  assert.equal((await gui.selectPolicy('auto','a')).ok,false);
  assert.equal((await gui.selectPolicy('manual','foreign')).ok,false);
  assert.equal(state.calls.length,0);
  assert.equal((await gui.selectPolicy('manual','auto')).ok,true);
  assert.equal(gui.policyGroups[0].selected,'auto');
});
test('rejected selection leaves state unchanged while accepted but unreadable selection is unconfirmed',async()=>{
  const {gui,state}=await harness();state.reject=true;
  assert.equal((await gui.selectPolicy('manual','b')).ok,false);assert.equal(gui.policyGroups[0].selected,'a');
  state.reject=false;state.readFailure=true;
  const result=await gui.selectPolicy('manual','b');
  assert.equal(result.ok,false);assert.match(result.message,/尚未确认/);
  assert.equal(state.groups[0].selected,'b');assert.equal(gui.policyGroups[0].selected,'a');
  assert.equal(gui.policyGroupsError,'query failed');assert.equal(gui.isSelectingPolicy,false);
});
test('failed TUN close preserves enabled and desired state; successful close leaves system proxy enabled',async()=>{
  const {gui,state}=await harness();state.reject=true;
  assert.equal((await gui.toggleTun()).ok,false);
  assert.equal(gui.isTunSwitchOn,true);assert.equal(gui.isSwitchingTun,false);
  state.reject=false;assert.equal((await gui.toggleTun()).ok,true);
  assert.equal(gui.isTunSwitchOn,false);assert.equal(gui.isSystemProxyEnabled,true);
  assert.deepEqual(state.calls,['tun-off','tun-off']);
});
test('system proxy failure is returned; successful close does not touch TUN',async()=>{
  const {gui,state}=await harness();state.reject=true;
  assert.equal((await gui.toggleSystemProxy()).ok,false);assert.equal(gui.isSystemProxyEnabled,true);
  state.reject=false;assert.equal((await gui.toggleSystemProxy()).ok,true);
  assert.equal(gui.isSystemProxyEnabled,false);assert.equal(gui.isTunSwitchOn,true);
});

test('accepted proxy command with failed readback reports uncertainty without success toast',async()=>{
  const {gui,state}=await harness();state.connectionReadFailure=true;
  const result=await gui.toggleSystemProxy();
  assert.equal(result.ok,false);assert.match(result.message,/尚未确认/);
  assert.equal(state.connection.systemProxyEnabled,false);
  assert.equal(state.notifications.some(([kind])=>kind==='success'),false);
  assert.equal(state.notifications.at(-1)[0],'warning');
});
test('enabled but unhealthy readback never confirms a successful TUN enable',async()=>{
  const {gui,state}=await harness();state.tun.enabled=state.tun.desiredEnabled=false;
  await gui.refreshTunStatus();state.tun.healthy=false;
  const result=await gui.toggleTun();assert.equal(result.ok,false);
  assert.equal(gui.isTunEnabled,true);assert.equal(gui.tunStatus.healthy,false);
  assert.equal(state.notifications.some(([kind])=>kind==='success'),false);
});
