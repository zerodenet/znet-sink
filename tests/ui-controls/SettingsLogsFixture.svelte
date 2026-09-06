<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import SettingsPanel from '$lib/components/SettingsPanel.svelte';
  import LogPanel from '$lib/components/core/LogPanel.svelte';
  import { store, type SettingsSection } from '$lib/services/store.svelte';
  const params = new URLSearchParams(location.search);
  store.settingsSection = (params.get('section') || 'general') as SettingsSection;
  store.uiMode = params.get('uiMode') === 'lite' ? 'lite' : 'pro';
  let showLogs = $state(params.get('panel') === 'logs');
</script>
<div class="fixture-navigation"><Button onclick={() => showLogs = !showLogs}>{showLogs ? '切到设置' : '切到日志'}</Button></div>
{#if showLogs}
  <div class="fixture-logs"><LogPanel /></div>
{:else}
  <SettingsPanel />
{/if}
<style>.fixture-navigation { margin-bottom:8px; }.fixture-logs { flex:1; min-height:0; }</style>
