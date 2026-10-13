<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import SettingsPanel from '$lib/components/SettingsPanel.svelte';
  import LogPanel from '$lib/components/core/LogPanel.svelte';
  import SystemProxyRecoveryDialog from '$lib/components/core/SystemProxyRecoveryDialog.svelte';
  import { guiState } from '$lib/services/gui-state.svelte';
  import { store, type SettingsSection } from '$lib/services/store.svelte';
  const params = new URLSearchParams(location.search);
  store.settingsSection = (params.get('section') || 'general') as SettingsSection;
  store.uiMode = params.get('uiMode') === 'lite' ? 'lite' : 'pro';
  if (params.get('mode') === 'endpoint-menu') {
    store.interactionSurface.navigation.set('endpoints', { key: 'endpoints', category: 'navigation', visible: params.has('menu-declared') && !params.has('menu-hidden'), operable: true, readonly: false });
  }
  let proxyEnabled = $state(true);
  let resetting = $state(false);
  let observed = $state(!params.has('status-unavailable'));
  $effect(() => {
    if (params.get('mode') !== 'network-rescue') return;
    Object.assign(guiState, {
      connection:{systemProxyEnabled:false,systemProxyActualEnabled:proxyEnabled,systemProxyHost:'127.0.0.1',systemProxyPort:7891,coreAvailable:false,processState:'stopped'},
      connectionError:observed ? null : '状态查询失败',
      isSystemProxyEnabled:proxyEnabled,hasUnmanagedSystemProxy:proxyEnabled,
      canResetSystemProxy:!resetting,canEnableSystemProxy:false,
      resetSystemProxy:async()=>{
        resetting=true;
        try {
          await new Promise<void>(resolve=>window.addEventListener('fixture-finish-reset',()=>resolve(),{once:true}));
          if (params.has('reset-failure')) return {ok:false,message:'系统拒绝修改代理设置'};
          proxyEnabled=false;observed=true;
          return {ok:true};
        } finally { resetting=false; }
      },
    });
  });
  let showLogs = $state(params.get('panel') === 'logs');
</script>
<div class="fixture-navigation"><Button onclick={() => showLogs = !showLogs}>{showLogs ? '切到设置' : '切到日志'}</Button></div>
{#if showLogs}
  <div class="fixture-logs"><LogPanel /></div>
{:else}
  <SettingsPanel />
{/if}
<SystemProxyRecoveryDialog />
<style>.fixture-navigation { margin-bottom:8px; }.fixture-logs { flex:1; min-height:0; }</style>
