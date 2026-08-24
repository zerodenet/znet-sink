<script lang="ts">
  import { store, type SettingsSection } from '$lib/services/store.svelte';
  import type { Component } from 'svelte';
  import { Spinner } from '$lib/components/ui/Spinner';
  import SectionWorkspace from '$lib/components/SectionWorkspace.svelte';

  let activeSection = $state(store.settingsSection);
  let ActivePanel = $state<Component | null>(null);
  let panelLoadError = $state<string | null>(null);

  const allSections: Array<{ id: SettingsSection; label: string }> = [
    { id: 'general', label: '应用' },
    { id: 'network', label: '网络' },
    { id: 'logs',    label: '日志' },
    { id: 'core',    label: '内核运行' },
    { id: 'dns',     label: 'DNS / Fake-IP' },
    { id: 'tun',     label: 'TUN 接管' },
    { id: 'config',  label: '高级配置' },
    { id: 'about',   label: '关于' },
  ];

  const sectionGroups: Record<SettingsSection, string> = {
    general: '客户端',
    network: '客户端',
    logs: '客户端',
    core: 'Zero 内核',
    dns: 'Zero 内核',
    tun: 'Zero 内核',
    config: 'Zero 内核',
    about: '其他',
  };

  const sections = $derived.by(() =>
    (store.uiMode === 'lite'
      ? allSections.filter((section) => section.id !== 'config' && section.id !== 'tun')
      : allSections
    ).map((section) => ({ ...section, group: sectionGroups[section.id] })),
  );

  const panelLoaders: Record<SettingsSection, () => Promise<{ default: Component }>> = {
    general: () => import('$lib/components/settings/GeneralSettingsPanel.svelte'),
    network: () => import('$lib/components/settings/NetworkSettingsPanel.svelte'),
    core: () => import('$lib/components/settings/CoreConfigPanel.svelte'),
    tun: () => import('$lib/components/settings/TunSettingsPanel.svelte'),
    dns: () => import('$lib/components/settings/DnsSettingsPanel.svelte'),
    config: () => import('$lib/components/settings/ConfigEditorPanel.svelte'),
    logs: () => import('$lib/components/settings/LogsSettingsPanel.svelte'),
    about: () => import('$lib/components/settings/AboutPanel.svelte'),
  };

  $effect(() => {
    if (store.activeTab === 'settings') {
      activeSection = store.settingsSection;
    }
  });

  // TUN parameters and the live kernel config editor are Pro-only settings.
  // Lite consumes the persisted TUN configuration without exposing another
  // configuration surface.
  $effect(() => {
    if (store.uiMode === 'lite' && (activeSection === 'config' || activeSection === 'tun')) {
      activeSection = 'general';
      store.settingsSection = 'general';
    }
  });

  $effect(() => {
    const section = activeSection;
    if (store.uiMode === 'lite' && (section === 'config' || section === 'tun')) return;

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
