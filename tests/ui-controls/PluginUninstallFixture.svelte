<script lang="ts">
  import MobileShell from '$lib/components/mobile/MobileShell.svelte';
  import { provideTrafficWorkspace } from '$lib/features/traffic/context';
  import { store } from './tun-state.svelte';
  import { mobilePlatform } from '$lib/features/mobile/platform.svelte';
  provideTrafficWorkspace();
  store.mobilePresentation = true;
  mobilePlatform.isMobileLayout = true;
  mobilePlatform.responsivePreview = true;
</script>
<MobileShell>
  {#snippet overview()}<p>插件宿主界面验证</p>{/snippet}
  {#snippet nodes()}<p>节点</p>{/snippet}
  {#snippet configuration()}<p>配置</p>{/snippet}
</MobileShell>
