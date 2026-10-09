<script lang="ts">
  import OverviewTab from '$lib/components/tabs/OverviewTab.svelte';
  import { guiState, store } from './tun-state.svelte';
  import { overviewData } from '$lib/services/overview-data.svelte';
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import type { ProxyMode, ConnectionStatus, PolicyGroup } from '$lib/types/gui-api';
  import type { GuiManagedTunStatus } from '$lib/types/tun';

  const scenario = new URLSearchParams(location.search).get('mode') ?? '';
  let ready = $state(false);
  let action = $state('');
  let mode = $state<ProxyMode>('rule');
  let updatedAt = $state(Date.now());
  let connection = $state<ConnectionStatus>({ state:'connected', processState:'running', coreAvailable:true, systemProxyEnabled:scenario !== 'partial', processPid:321, localProxyHost:'127.0.0.1', localProxyPort:7890 });
  let tun = $state<GuiManagedTunStatus>({ key:'tun', supported:true, enabled:scenario !== 'desired', desiredEnabled:true, healthy:!['failure','recovery-rejected','recovery-error'].includes(scenario), state:'running', addresses:[], lastError:['failure','recovery-rejected','recovery-error'].includes(scenario) ? '出口恢复失败' : undefined,
    autoRoute:true,dualStack:true,strictRoute:true,dnsHijack:false,fakeIpEnabled:false,dnsHijackedQueries:0,networkGeneration:1,ipv6ToIpv4Fallbacks:0,managedByConfig:false,ipv4Egress:{availability:'available'},addressFamilyPolicy:scenario === 'ipv6-required' ? 'ipv6_only' : 'prefer_ipv4',ipv6Egress:{availability:scenario === 'ipv4-only-network' || scenario === 'ipv6-required' ? 'unavailable' : 'available'},
  });
  const selfTest = { ready:true, activeProxyConfigId:'main', activeProxyConfigName:'日常网络配置', checks:[], blockingIssues:[], warningCount:0, suggestedFlow:'ready' };
  let groups = $state<PolicyGroup[]>(scenario === 'shared-probe' ? [
    {name:'YouTube',kind:'selector',selected:'节点选择',outbounds:[{tag:'节点选择',type:'selector'}]},
    {name:'节点选择',kind:'selector',selected:'自动选择',outbounds:[{tag:'自动选择',type:'urltest'}]},
    {name:'AI Suite',kind:'selector',selected:'US',outbounds:[{tag:'US',type:'unknown'}]},
    {name:'自动选择',kind:'urltest',selected:'US',outbounds:[{tag:'US',type:'unknown',alive:true,delayMs:356,lastCheckedUnixMs:Date.now()},{tag:'JP',type:'unknown',alive:true,delayMs:20,lastCheckedUnixMs:Date.now()}]},
  ] : [{ name:'proxy', kind:'selector', selected:'测试节点', outbounds:[{ tag:'测试节点', type:'unknown', alive:true, delayMs:25, lastCheckedUnixMs:Date.now() }] }]);
  async function disconnect() { action='disconnect'; connection.systemProxyEnabled=false; tun.enabled=false; tun.desiredEnabled=false; tun.lastError=undefined; updatedAt=Date.now(); }
  async function connect() { action='connect'; connection.systemProxyEnabled=true; tun.enabled=true; tun.desiredEnabled=true; updatedAt=Date.now(); }
  async function setProxyMode(next: ProxyMode) {
    if (scenario === 'mode-failure') return {ok:false as const,message:'模式请求已提交，但尚未确认生效，请重新检查'};
    mode=next; action=`mode:${next}`; return { ok:true as const };
  }
  let recoveryRequests = 0;
  async function recoverTun(options?: { notify?: boolean }) {
    action = 'tun-recover';
    guiState.isSwitchingTun = true;
    window.dispatchEvent(new CustomEvent('fixture-recovery-request', {detail:{count:++recoveryRequests,options}}));
    try {
      if (new URLSearchParams(location.search).has('recovery-pending')) {
        await new Promise<void>(resolve => window.addEventListener('fixture-finish-recovery', () => resolve(), {once:true}));
      }
      if (scenario === 'recovery-rejected') return {ok:false,message:'出口恢复失败，请检查本地网络'};
      if (scenario === 'recovery-error') throw new Error('控制接口暂时不可用');
      tun.healthy=true; tun.lastError=undefined; updatedAt=Date.now();
      return {ok:true,message:'TUN 路由检查通过'};
    } finally { guiState.isSwitchingTun = false; }
  }
  function sample(at = Date.now()) {
    overviewData.applyTrafficRateSample({ sampledAtUnixMs:at, stable:true, uploadBytesPerSec:80000, downloadBytesPerSec:500000, totalUploadBytes:12000000, totalDownloadBytes:320000000, connectionCount:4 });
  }
  $effect(() => {
    Object.assign(guiState, { connection, connectionUpdatedAt:updatedAt, connectionError:null, coreOverview:{coreState:'running'},
      selfTest, selfTestUpdatedAt:updatedAt, tunStatus:tun, tunStatusError:null,
      proxyMode:{currentMode:mode,availableModes:scenario === 'limited' ? ['rule','direct'] : ['rule','global','direct']},
      policyGroups:groups, policyGroupsUpdatedAt:updatedAt, policyGroupsError:null, configNodes:[],
      isTunEnabled:tun.enabled,isTunDesiredEnabled:tun.desiredEnabled,isTunSwitchOn:tun.enabled || tun.desiredEnabled,
      isSystemProxyEnabled:connection.systemProxyEnabled,isCaptureEnabled:connection.systemProxyEnabled || tun.enabled,isConnected:connection.systemProxyEnabled && tun.enabled,
      isInitializing:false,isCoreBusy:false,isStartingCore:false,isStoppingCore:false,isConnecting:false,isDisconnecting:false,isSwitchingTun:false,isSwitchingSystemProxy:false,isSwitchingMode:scenario === 'busy',isSelectingPolicy:false,
      supportsTrafficStats:true,canConnect:true,canDisconnect:true,canRestartCore:true,canStartCore:true,canDisableSystemProxy:true,canEnableSystemProxy:true,canDisableTun:true,canEnableTun:true,
      connect,disconnect,setProxyMode,
      refreshSelfTest:async()=>{},refreshNodeStateAfterConfigChange:async()=>{},refreshAll:async()=>{},
      restartCore:async()=>{action='restart';},startCore:async()=>{action='start';},
      toggleSystemProxy:async()=>{connection.systemProxyEnabled=!connection.systemProxyEnabled;return {ok:true};},
      recoverTun,
      toggleTun:async()=>{tun.enabled=!tun.enabled;tun.desiredEnabled=tun.enabled;return {ok:true};},
    });
    ready=true;
  });
  onMount(() => {
    const applyPolicies = (event: Event) => {
      groups = (event as CustomEvent<PolicyGroup[]>).detail;
      updatedAt=Date.now();
    };
    const applySample = (event: Event) => {
      const {up,down,upRate,downRate} = (event as CustomEvent<{up:number;down:number;upRate:number;downRate:number}>).detail;
      updatedAt=Date.now();
      overviewData.applyTrafficRateSample({sampledAtUnixMs:updatedAt,stable:true,uploadBytesPerSec:upRate,downloadBytesPerSec:downRate,totalUploadBytes:up,totalDownloadBytes:down,connectionCount:4});
    };
    window.addEventListener('fixture-traffic-sample',applySample);
    window.addEventListener('fixture-policies',applyPolicies);
    store.uiMode='lite';sample();
    overviewData.beginCaptureSession();
    overviewData.applyTrafficRateSample({sampledAtUnixMs:Date.now(),stable:true,uploadBytesPerSec:80000,downloadBytesPerSec:500000,totalUploadBytes:13000000,totalDownloadBytes:322000000,connectionCount:4});
    return () => {
      window.removeEventListener('fixture-traffic-sample',applySample);
      window.removeEventListener('fixture-policies',applyPolicies);
    };
  });
</script>
<div class="flex gap-2">
  <Button onclick={()=>{store.uiMode='lite';}}>简约视图</Button>
  <Button onclick={()=>{store.uiMode='pro';}}>专业视图</Button>
  <Button onclick={()=>{updatedAt=Date.now()-20000;}}>状态过期</Button>
  <Button onclick={()=>{sample(Date.now()-11000);}}>采样过期</Button>
</div>
{#if ready}<OverviewTab />{/if}
<output aria-label="模式操作">{action}</output>
<output aria-label="会话累计">{overviewData.captureSessionTotalBytes}</output>
