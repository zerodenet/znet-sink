<script lang="ts">
  import MobileOverview from '$lib/components/mobile/MobileOverview.svelte';
  import MobileShell from '$lib/components/mobile/MobileShell.svelte';
  import MobileConfiguration from '$lib/components/mobile/MobileConfiguration.svelte';
  import NodesTab from '$lib/components/tabs/NodesTab.svelte';
  import {provideTrafficWorkspace} from '$lib/features/traffic/context';
  import {guiState,store} from './tun-state.svelte';
  import {mobilePlatform} from '$lib/features/mobile/platform.svelte';
  import {mobileLocationFixture} from './mobile-location';
  import MobileLocationObserver from './mobile-location-observer.svelte';
  provideTrafficWorkspace();
  store.activeTab='overview';store.mobilePresentation=true;
  mobilePlatform.isMobileLayout=true;mobilePlatform.responsivePreview=true;
  const currentMode=new URLSearchParams(location.search).get('mode')??'global';
  Object.assign(guiState,{
    connection:{coreAvailable:!new URLSearchParams(location.search).has('offline')},connectionUpdatedAt:Date.now(),connectionError:null,
    proxyMode:{currentMode,availableModes:['rule','direct','global'],globalOutbound:new URLSearchParams(location.search).get('target')??'outer-auto'},
    configNodes:[{tag:'node-a'},{tag:'node-b'},{tag:'independent'}],
    configPolicyGroups:[{name:'outer-auto',kind:'urltest'},{name:'inner-auto',kind:'fallback'},{name:'manual',kind:'selector'}],
    policyGroups:[
      {name:'outer-auto',kind:'urltest',selected:'inner-auto',outbounds:[{tag:'inner-auto'},{tag:'node-b'}]},
      {name:'inner-auto',kind:'fallback',selected:'node-a',outbounds:[{tag:'node-a'},{tag:'node-b'}]},
      {name:'manual',kind:'selector',selected:'node-b',outbounds:[{tag:'node-a'},{tag:'node-b'}]},
    ],
  });
  // Keep the explicitly imported host-fact module in this actual component fixture.
  void mobileLocationFixture;
</script>
<MobileShell>
  {#snippet overview()}<MobileLocationObserver/><MobileOverview />{/snippet}
  {#snippet nodes()}<NodesTab mobile={true}/>{/snippet}
  {#snippet configuration()}<MobileConfiguration />{/snippet}
</MobileShell>
