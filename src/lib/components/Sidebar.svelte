<script lang="ts">
  import { guiState } from '$lib/services/gui-state.svelte';

  // Initial TUN status fetch when initialized
  $effect(() => {
    if (guiState.connection !== null) {
      guiState.refreshTunStatus();
    }
  });

  async function toggleSystemProxy() {
    if (guiState.isSwitchingSystemProxy || guiState.isConnecting || guiState.isDisconnecting) return;
    await guiState.toggleSystemProxy();
  }

  async function toggleTun() {
    if (guiState.isSwitchingTun || guiState.isConnecting || guiState.isDisconnecting) return;
    await guiState.toggleTun();
  }

  const isCoreRunning = $derived(guiState.isProcessRunning);
  const proxyEnabled = $derived(guiState.isSystemProxyEnabled);
  const isTunActive = $derived(guiState.isTunEnabled);
</script>

<aside class="w-14 h-full bg-card border-r border-border flex flex-col items-center py-4 justify-between flex-shrink-0 hidden sm:flex">
  <!-- 上部：品牌 & 内核状态 -->
  <div class="flex flex-col items-center gap-4">
    <div class="w-7 h-7 rounded-lg bg-muted flex items-center justify-center font-bold text-foreground text-xs border border-border">
      Z
    </div>

    <!-- 内核状态指示 -->
    <button data-slot="surface-button"
      class="w-8 h-8 rounded-xl flex items-center justify-center border text-base transition-all duration-200
             {isCoreRunning ? 'bg-success/10 border-success/30 text-success' : 'bg-muted border-border text-muted-foreground hover:text-muted-foreground'}"
      title={isCoreRunning ? "内核监听中" : "内核未运行"}
      disabled
    >
      {isCoreRunning ? '●' : '○'}
    </button>
  </div>

  <!-- 下部：TUN / SYS 快捷操作 -->
  <div class="flex flex-col gap-2">
    <button data-slot="surface-button"
      onclick={toggleTun}
      disabled={isTunActive ? !guiState.canDisableTun : !guiState.canEnableTun}
      class="w-7 h-7 rounded-lg text-[10px] font-mono font-bold border transition-colors duration-150
             {isTunActive
               ? 'bg-success/10 border-success/30 text-success'
               : isCoreRunning
                 ? 'bg-muted border-border text-muted-foreground hover:text-muted-foreground'
                 : 'bg-muted border-border text-muted-foreground'}"
      title={isTunActive ? "TUN 已开启" : "TUN 未开启"}
    >
      {guiState.isSwitchingTun ? '…' : 'TUN'}
    </button>
    <button data-slot="surface-button"
      onclick={toggleSystemProxy}
      disabled={proxyEnabled ? !guiState.canDisableSystemProxy : !guiState.canEnableSystemProxy}
      class="w-7 h-7 rounded-lg text-[10px] font-mono font-bold border transition-colors duration-150
             {proxyEnabled
               ? 'bg-success/10 border-success/30 text-success'
               : 'bg-muted border-border text-muted-foreground hover:text-muted-foreground'}"
      title={proxyEnabled ? "系统代理已开启" : "系统代理未开启"}
    >
      {guiState.isSwitchingSystemProxy ? '…' : 'SYS'}
    </button>
  </div>
</aside>
