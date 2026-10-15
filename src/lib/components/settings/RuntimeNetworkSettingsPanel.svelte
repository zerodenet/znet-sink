<script lang="ts">
 import {mobilePlatform} from '$lib/features/mobile/platform.svelte';
 import RuntimeNetworkEditor from './RuntimeNetworkEditor.svelte';
 import LegacyRuntimeNetworkSettingsPanel from './LegacyRuntimeNetworkSettingsPanel.svelte';
</script>
{#if mobilePlatform.isMobileLayout}<RuntimeNetworkEditor/>{:else}<LegacyRuntimeNetworkSettingsPanel/>{/if}
