<script lang="ts">
 import {onMount} from 'svelte';
 import MobileShell from '$lib/components/mobile/MobileShell.svelte';
 import MobileShellPageFixture from './MobileShellPageFixture.svelte';
 import {store} from '$lib/services/store.svelte';
 import {observeMobilePresentation} from '$lib/features/mobile/platform.svelte';
 store.activeTab='overview';store.uiMode='lite';store.mobilePresentation=true;
 onMount(observeMobilePresentation);
</script>
<MobileShell>{#snippet overview()}<MobileShellPageFixture name="概览"/>{/snippet}{#snippet nodes()}<MobileShellPageFixture name="节点"/>{/snippet}{#snippet configuration()}<MobileShellPageFixture name="配置"/>{/snippet}</MobileShell>
