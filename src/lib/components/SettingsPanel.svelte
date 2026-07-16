<script lang="ts">
  import { store, type SettingsSection } from '$lib/services/store.svelte';
  import type { Component } from 'svelte';
  import { Spinner } from '$lib/components/ui/Spinner';

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

<section class="settings-root animate-fade-in">
  <!-- Left: sidebar nav -->
  <nav class="settings-nav" aria-label="设置导航">
    <div class="settings-nav-header">设置</div>
    {#each sections as section}
      <button
        onclick={() => {
          activeSection = section.id;
          store.settingsSection = section.id;
        }}
        class="settings-nav-item {activeSection === section.id ? 'active' : ''}"
      >
        {section.label}
      </button>
    {/each}
  </nav>

  <!-- Right: content -->
  <div class="settings-content">
    {#if panelLoadError}
      <div class="panel-loading error">设置页面加载失败：{panelLoadError}</div>
    {:else if ActivePanel}
      <ActivePanel />
    {:else}
      <div class="panel-loading"><Spinner size="sm" color="default" />正在加载…</div>
    {/if}
  </div>
</section>

<style>
  .settings-root {
    flex: 1;
    width: 100%;
    display: flex;
    gap: 0;
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
    overflow: hidden;
    transition: box-shadow 0.2s ease;
  }

  /* ---- Sidebar nav ---- */
  .settings-nav {
    width: min(130px, 30vw);
    min-width: 80px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 14px 8px;
    border-right: 1px solid var(--border);
    background: var(--surface, rgba(0,0,0,0.018));
  }

  :global(.dark) .settings-nav {
    background: rgba(255, 255, 255, 0.015);
  }

  .settings-nav-header {
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted-foreground);
    padding: 4px 8px 10px;
    opacity: 0.65;
  }

  .settings-nav-item {
    display: flex;
    align-items: center;
    width: 100%;
    padding: 7px 10px;
    border-radius: 6px;
    border: none;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    text-align: left;
    transition: background 0.13s ease, color 0.13s ease;
  }

  .settings-nav-item:hover {
    background: var(--muted);
    color: var(--foreground);
  }

  .settings-nav-item.active {
    background: var(--primary);
    color: var(--primary-foreground);
    font-weight: 600;
  }

  /* ---- Content area ---- */
  .settings-content {
    flex: 1;
    overflow-y: auto;
    padding: 14px 16px;
    min-height: 0;
  }

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

  @media (max-width: 500px) {
    .settings-content {
      padding: 10px 12px;
    }
    .settings-nav-item {
      padding: 6px 6px;
      font-size: 11px;
    }
  }

</style>
