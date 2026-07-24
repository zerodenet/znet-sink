<script lang="ts">
  import { store, type SettingsSection } from '$lib/services/store.svelte';
  import type { Component } from 'svelte';
  import { Spinner } from '$lib/components/ui/Spinner';
  import SectionWorkspace from '$lib/components/SectionWorkspace.svelte';

  let activeSection = $state(store.settingsSection);
  let ActivePanel = $state<Component | null>(null);
  let panelLoadError = $state<string | null>(null);

  $effect(() => {
    if (store.activeTab === 'settings') {
      activeSection = store.settingsSection;
    }
  });

  const sections: Array<{ id: SettingsSection; label: string }> = [
    { id: 'general', label: '通用' },
    { id: 'core',    label: '内核' },
    { id: 'config',  label: '配置' },
    { id: 'about',   label: '关于' }
  ];

  const panelLoaders: Record<SettingsSection, () => Promise<{ default: Component }>> = {
    general: () => import('$lib/components/settings/AppConfigPanel.svelte'),
    core: () => import('$lib/components/settings/CoreConfigPanel.svelte'),
    config: () => import('$lib/components/settings/ConfigEditorPanel.svelte'),
    about: () => import('$lib/components/settings/AboutPanel.svelte'),
  };

  $effect(() => {
    const section = activeSection;
    ActivePanel = null;
    panelLoadError = null;
    void panelLoaders[section]()
      .then((module) => {
        if (activeSection === section) ActivePanel = module.default;
      })
      .catch((error) => {
        if (activeSection === section) {
          panelLoadError = error instanceof Error ? error.message : String(error);
        }
      });
  });
</script>

<SectionWorkspace
  title="设置"
  items={sections}
  activeId={activeSection}
  onSelect={(id) => {
    activeSection = id as SettingsSection;
    store.settingsSection = activeSection;
  }}
>
  {#if panelLoadError}
    <div class="panel-loading error">设置页面加载失败：{panelLoadError}</div>
  {:else if ActivePanel}
    <ActivePanel />
  {:else}
    <div class="panel-loading"><Spinner size="sm" color="default" />正在加载…</div>
  {/if}
</SectionWorkspace>

<style>
  .panel-loading {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--muted-foreground);
    font-size: 12px;
  }

  .panel-loading.error {
    color: var(--destructive);
    text-align: center;
    overflow-wrap: anywhere;
  }

</style>
