<script lang="ts">
  import { getAppConfig, updateAppConfig } from '$lib/services/core';
  import { store } from '$lib/services/store.svelte';
  import { setTheme, type ThemeMode } from '$lib/services/theme.svelte';
  import type { AppConfig } from '$lib/types/app-config';
  import { Switch } from '$lib/components/ui/switch';

  let config = $state<AppConfig | null>(null);
  let loading = $state(false);

  async function refreshConfig() {
    try {
      config = await getAppConfig();
    } catch (e) {
      console.error('Failed to get app config:', e);
    }
  }

  async function toggleCoreSetting(key: 'autoStart' | 'autoConnect') {
    if (!config) return;
    loading = true;
    try {
      const current = config.core[key];
      const updated = await updateAppConfig({ core: { [key]: !current } });
      config = updated;
    } catch (e) {
      console.error('Failed to update config:', e);
    } finally {
      loading = false;
    }
  }

  function handleThemeChange(theme: ThemeMode) {
    setTheme(theme);
  }

  $effect(() => {
    refreshConfig();
  });
</script>

<div class="bg-card border border-card-border rounded-xl p-4">
  <h3 class="text-sm font-bold text-foreground mb-4">应用配置</h3>

  {#if !config}
    <div class="text-xs text-muted-foreground py-4">加载中...</div>
  {:else}
    <div class="space-y-4">
      <!-- 主题设置 -->
      <div class="flex items-center justify-between py-1">
        <span class="text-xs text-muted-foreground">主题</span>
        <div class="flex bg-muted rounded-xl p-0.5 text-[10px] font-bold shadow-inner">
          {#each ['light', 'dark', 'system'] as theme}
            <button
              onclick={() => handleThemeChange(theme as ThemeMode)}
              class="px-3 py-1.5 rounded-lg transition-all duration-200
                     {store.selectedTheme === theme 
                       ? 'bg-primary text-primary-foreground shadow-sm' 
                       : 'text-muted-foreground hover:text-foreground hover:bg-muted-foreground/10'}"
            >
              {theme === 'light' ? '明亮' : theme === 'dark' ? '暗黑' : '跟随系统'}
            </button>
          {/each}
        </div>
      </div>

      <!-- UI模式 -->
      <div class="flex items-center justify-between py-1">
        <span class="text-xs text-muted-foreground">界面模式</span>
        <div class="flex bg-muted rounded-xl p-0.5 text-[10px] font-bold shadow-inner">
          <button
            onclick={async () => await store.switchUIMode('lite')}
            class="px-3 py-1.5 rounded-lg transition-all duration-200
                   {store.uiMode === 'lite' 
                     ? 'bg-primary text-primary-foreground shadow-sm' 
                     : 'text-muted-foreground hover:text-foreground hover:bg-muted-foreground/10'}"
          >
            简约
          </button>
          <button
            onclick={async () => await store.switchUIMode('pro')}
            class="px-3 py-1.5 rounded-lg transition-all duration-200
                   {store.uiMode === 'pro' 
                     ? 'bg-primary text-primary-foreground shadow-sm' 
                     : 'text-muted-foreground hover:text-foreground hover:bg-muted-foreground/10'}"
          >
            专业
          </button>
        </div>
      </div>

      <!-- 自动启动内核 -->
      <div class="flex items-center justify-between py-1">
        <span class="text-xs text-muted-foreground">开机自动启动内核</span>
        <Switch
          checked={config.core.autoStart}
          onCheckedChange={() => toggleCoreSetting('autoStart')}
          disabled={loading}
          aria-label="开机自动启动内核"
        />
      </div>

      <!-- 自动连接 -->
      <div class="flex items-center justify-between py-1">
        <span class="text-xs text-muted-foreground">启动后自动连接</span>
        <Switch
          checked={config.core.autoConnect}
          onCheckedChange={() => toggleCoreSetting('autoConnect')}
          disabled={loading}
          aria-label="启动后自动连接"
        />
      </div>

      <!-- 页面可见性 -->
      <!-- 页面可见性由 Rust 后端 InteractionSurfaceSnapshot 控制 -->
      <!-- 用户偏好设置需通过后端 API 更新后自动同步 -->
    </div>
  {/if}
</div>
