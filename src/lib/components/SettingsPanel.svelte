<script lang="ts">
  import { store } from '$lib/services/store.svelte';
  import { NAV_TABS } from '$lib/constants/navigation';
  import AppConfigPanel from '$lib/components/settings/AppConfigPanel.svelte';
  import CoreConfigPanel from '$lib/components/settings/CoreConfigPanel.svelte';
  import CapabilitiesTab from '$lib/components/tabs/CapabilitiesTab.svelte';

  let activeSection = $state('general');

  const sections = [
    { id: 'general', label: '通用' },
    { id: 'core', label: '内核' },
    { id: 'plugins', label: '插件' },
    { id: 'about', label: '关于' }
  ];
</script>

<section class="flex-1 w-full bg-card border border-card-border rounded-xl p-6 shadow-sm flex gap-8 animate-fade-in transition-colors duration-300 overflow-hidden">
  <!-- 左侧导航菜单 -->
  <nav class="w-32 flex flex-col gap-0.5 flex-shrink-0">
    <div class="text-[10px] font-mono font-black text-muted-foreground px-3 py-2 uppercase tracking-widest">设置</div>
    {#each sections as section}
      <button
        onclick={() => activeSection = section.id}
        class="text-left px-3 py-2 rounded-xl text-xs font-bold transition-all
               {activeSection === section.id 
                 ? 'bg-primary text-primary-foreground shadow-sm' 
                 : 'text-muted-foreground hover:text-foreground hover:bg-muted/50'}"
      >
        {section.label}
      </button>
    {/each}
  </nav>

  <!-- 右侧内容区域 -->
  <div class="flex-1 overflow-y-auto pr-2">
    {#if activeSection === 'general'}
      <div class="flex flex-col gap-4">
        <AppConfigPanel />
      </div>
    {:else if activeSection === 'core'}
      <div class="flex flex-col gap-4">
        <CoreConfigPanel />
      </div>
    {:else}
      <div class="text-center py-8">
        <div class="text-xs text-muted-foreground">开发中...</div>
      </div>
    {/if}
  </div>
</section>
