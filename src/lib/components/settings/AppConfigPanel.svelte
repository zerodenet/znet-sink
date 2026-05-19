<script lang="ts">
  import { getAppConfig, updateAppConfig } from '$lib/services/core';
  import { store } from '$lib/services/store.svelte';
  import { applyTheme, type ThemeMode } from '$lib/services/theme.svelte';

  let config = $state<Record<string, unknown> | null>(null);
  let loading = $state(false);

  async function refreshConfig() {
    try {
      config = await getAppConfig();
    } catch (e) {
      console.error('Failed to get app config:', e);
    }
  }

  async function handleChange(key: string, value: unknown) {
    if (!config) return;
    loading = true;
    try {
      const newConfig = { ...config, [key]: value };
      await updateAppConfig(newConfig);
      config = newConfig;
    } catch (e) {
      console.error('Failed to update config:', e);
    } finally {
      loading = false;
    }
  }

  function getCoreValue(cfg: Record<string, unknown>, key: string): boolean {
    const core = cfg.core as Record<string, unknown> | undefined;
    return Boolean(core?.[key]);
  }

  $effect(() => {
    refreshConfig();
  });
</script>

<div class="bg-card border border-card-border rounded-xl p-4">
  <h3 class="text-sm font-bold text-foreground mb-4">应用配置</h3>
  
  {#if !config}
    <div class="text-xs text-muted-foreground">加载中...</div>
  {:else}
    <div class="space-y-4">
      <!-- 主题设置 -->
      <div class="flex items-center justify-between">
        <span class="text-xs text-muted-foreground">主题</span>
        <div class="flex bg-muted rounded-lg p-0.5 text-[10px] font-bold">
          {#each ['light', 'dark', 'system'] as theme}
            <button
              onclick={() => {
                const themeMode = theme as ThemeMode;
                store.selectedTheme = themeMode;
                applyTheme(themeMode);
              }}
              class="px-3 py-1 rounded-md transition-all {store.selectedTheme === theme ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground'}"
            >
              {theme === 'light' ? '明亮' : theme === 'dark' ? '暗黑' : '跟随系统'}
            </button>
          {/each}
        </div>
      </div>

      <!-- UI模式 -->
      <div class="flex items-center justify-between">
        <span class="text-xs text-muted-foreground">界面模式</span>
        <div class="flex bg-muted rounded-lg p-0.5 text-[10px] font-bold">
          <button
            onclick={() => store.switchUIMode('lite')}
            class="px-3 py-1 rounded-md transition-all {store.uiMode === 'lite' ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground'}"
          >
            简约
          </button>
          <button
            onclick={() => store.switchUIMode('pro')}
            class="px-3 py-1 rounded-md transition-all {store.uiMode === 'pro' ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground'}"
          >
            专业
          </button>
        </div>
      </div>

      <!-- 自动启动内核 -->
      <div class="flex items-center justify-between">
        <span class="text-xs text-muted-foreground">开机自动启动内核</span>
        <button
          onclick={() => {
            const currentValue = getCoreValue(config!, 'auto_start');
            handleChange('core', { ...(config!.core as object ?? {}), auto_start: !currentValue });
          }}
          disabled={loading}
          class="w-9 h-5 rounded-full relative transition-colors disabled:opacity-50 {getCoreValue(config!, 'auto_start') ? 'bg-primary' : 'bg-muted'}"
          aria-label={getCoreValue(config!, 'auto_start') ? '关闭开机自动启动' : '开启开机自动启动'}
          title={getCoreValue(config!, 'auto_start') ? '关闭开机自动启动' : '开启开机自动启动'}
        >
          <div class="w-4 h-4 rounded-full bg-white absolute top-0.5 transition-all shadow {getCoreValue(config!, 'auto_start') ? 'left-4' : 'left-0.5'}"></div>
        </button>
      </div>

       <!-- TUN模式 -->
      <div class="flex items-center justify-between">
        <span class="text-xs text-muted-foreground">TUN虚拟网卡模式</span>
         <button
          onclick={() => {
            const currentValue = getCoreValue(config!, 'tun_mode');
            handleChange('core', { ...(config!.core as object ?? {}), tun_mode: !currentValue });
          }}
          disabled={loading}
          class="w-9 h-5 rounded-full relative transition-colors disabled:opacity-50 {getCoreValue(config!, 'tun_mode') ? 'bg-primary' : 'bg-muted'}"
          aria-label={getCoreValue(config!, 'tun_mode') ? '关闭TUN模式' : '开启TUN模式'}
          title={getCoreValue(config!, 'tun_mode') ? '关闭TUN模式' : '开启TUN模式'}
        >
          <div class="w-4 h-4 rounded-full bg-white absolute top-0.5 transition-all shadow {getCoreValue(config!, 'tun_mode') ? 'left-4' : 'left-0.5'}"></div>
        </button>
      </div>

      <!-- 页面可见性 -->
      <div>
        <div class="text-xs text-muted-foreground mb-2">页面可见性</div>
        <div class="flex flex-wrap gap-1">
          {#each [
            { id: 'overview', label: '概览' },
            { id: 'profiles', label: '配置' },
            { id: 'subscriptions', label: '订阅' },
            { id: 'rules', label: '规则' },
            { id: 'connections', label: '连接' },
            { id: 'logs', label: '日志' },
            { id: 'settings', label: '设置' }
          ] as tab}
            <button
              onclick={() => tab.id !== 'settings' && store.toggleTabVisibility(tab.id)}
              class="px-2 py-1 rounded text-[10px] font-bold transition-all whitespace-nowrap
                     {store.visibleTabs.includes(tab.id) 
                       ? 'bg-primary text-primary-foreground shadow-sm' 
                       : 'bg-muted text-muted-foreground hover:text-foreground'}
                     {tab.id === 'settings' ? 'cursor-not-allowed' : ''}"
            >
              {tab.label}
            </button>
          {/each}
        </div>
      </div>
    </div>
  {/if}
</div>
