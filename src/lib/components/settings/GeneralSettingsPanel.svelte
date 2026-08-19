<script lang="ts">
  import AppConfigPanel from '$lib/components/settings/AppConfigPanel.svelte';
  import LocalProxyEndpointPanel from '$lib/components/settings/LocalProxyEndpointPanel.svelte';
  import TrafficBallSettingsPanel from '$lib/components/settings/TrafficBallSettingsPanel.svelte';
  import UrlTestSettingsPanel from '$lib/components/settings/UrlTestSettingsPanel.svelte';
</script>

<TrafficBallSettingsPanel />
<LocalProxyEndpointPanel />
<UrlTestSettingsPanel />
<AppConfigPanel />
