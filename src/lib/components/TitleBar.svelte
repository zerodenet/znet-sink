<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { getName, getVersion } from '@tauri-apps/api/app';
  import { store } from '$lib/services/store.svelte';

  let appWindow: ReturnType<typeof getCurrentWindow> | null = null;
  let appName = $state('');
  let appVersion = $state('');

  $effect(() => {
    let mounted = true;
    async function init() {
      try {
        appWindow = getCurrentWindow();
        if (mounted) {
          appName = await getName();
          appVersion = await getVersion();
        }
      } catch (e) {
        // 非 Tauri 环境下忽略
      }
    }
    init();
    return () => { mounted = false; };
  });

  const handleMinimize = () => appWindow?.minimize().catch(() => {});
  const handleMaximize = () => appWindow?.toggleMaximize().catch(() => {});
  const handleClose = () => appWindow?.close().catch(() => {});
</script>

<div data-tauri-drag-region class="h-10 w-full flex items-center justify-between px-5 bg-background border-b border-card-border flex-shrink-0">
  <div class="flex items-center gap-3">
    <img src="/favicon.png" alt="Logo" class="w-4 h-4 rounded" />
    <span class="font-black text-foreground/90 text-xs tracking-wider">{appName}</span>
    <span class="text-[10px] text-muted-foreground">v{appVersion}</span>
    
    <!-- 模式切换 -->
    <div class="relative flex bg-muted rounded-md p-0.5 gap-0.5" style="font-size: 10px;">
      <!-- 滑动指示器 -->
      <div
        class="absolute top-0.5 bottom-0.5 rounded-md bg-primary shadow-sm transition-all duration-200 ease-out"
        style="width: 36px; left: {store.uiMode === 'lite' ? '2px' : '42px'}"
      ></div>
      <button
        onclick={() => store.switchUIMode('lite')}
        class="relative z-10 w-9 text-center py-0.5 rounded-md font-bold transition-colors duration-200 {store.uiMode === 'lite' ? 'text-primary-foreground' : 'text-muted-foreground hover:text-foreground'}"
        aria-label="切换到简约模式"
        title="切换到简约模式"
      >
        简约
      </button>
      <button
        onclick={() => store.switchUIMode('pro')}
        class="relative z-10 w-9 text-center py-0.5 rounded-md font-bold transition-colors duration-200 {store.uiMode === 'pro' ? 'text-primary-foreground' : 'text-muted-foreground hover:text-foreground'}"
        aria-label="切换到专业模式"
        title="切换到专业模式"
      >
        专业
      </button>
    </div>
  </div>
  
  <div class="flex items-center gap-1">
    <button onclick={handleMinimize} class="w-6 h-6 rounded flex items-center justify-center hover:bg-muted text-muted-foreground hover:text-foreground transition-colors" aria-label="最小化窗口" title="最小化">
      <svg width="10" height="10" viewBox="0 0 10 10" fill="currentColor"><rect x="1" y="5" width="8" height="1"/></svg>
    </button>
    <button onclick={handleMaximize} class="w-6 h-6 rounded flex items-center justify-center hover:bg-muted text-muted-foreground hover:text-foreground transition-colors" aria-label="最大化/还原窗口" title="最大化">
      <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1"><rect x="1.5" y="1.5" width="7" height="7"/></svg>
    </button>
    <button onclick={handleClose} class="w-6 h-6 rounded flex items-center justify-center hover:bg-red-500 hover:text-white text-muted-foreground transition-colors" aria-label="关闭窗口" title="关闭">
      <svg width="10" height="10" viewBox="0 0 10 10" stroke="currentColor" stroke-width="1.5"><line x1="2" y1="2" x2="8" y2="8"/><line x1="8" y1="2" x2="2" y2="8"/></svg>
    </button>
  </div>
</div>
