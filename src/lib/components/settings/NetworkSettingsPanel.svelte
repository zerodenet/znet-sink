<script lang="ts">
  import LocalProxyEndpointPanel from '$lib/components/settings/LocalProxyEndpointPanel.svelte';
  import AppConfigPanel from '$lib/components/settings/AppConfigPanel.svelte';
  import UrlTestSettingsPanel from '$lib/components/settings/UrlTestSettingsPanel.svelte';
</script>

<LocalProxyEndpointPanel />
<AppConfigPanel scope="network" />
<div class="section-gap"></div>
<UrlTestSettingsPanel />

<style>
  .section-gap {
    height: 1px;
    margin: 16px 0;
    background: var(--border);
  }
</style>
