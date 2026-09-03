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
  const isTunActive = $derived(guiState.isTunEnabled && !guiState.tunStatusError);
  const tunUnconfirmed = $derived(!!guiState.tunStatusError || (guiState.isTunDesiredEnabled && !guiState.isTunEnabled));
</script>

<aside class="w-14 h-full bg-[#121418] border-r border-zinc-800/40 flex flex-col items-center py-4 justify-between flex-shrink-0 hidden sm:flex">
  <!-- 上部：品牌 & 内核状态 -->
  <div class="flex flex-col items-center gap-4">
    <div class="w-7 h-7 rounded-lg bg-zinc-800 flex items-center justify-center font-bold text-zinc-200 text-xs border border-zinc-700/50">
      Z
    </div>

    <!-- 内核状态指示 -->
    <button
      class="w-8 h-8 rounded-xl flex items-center justify-center border text-base transition-all duration-200
             {isCoreRunning ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400' : 'bg-zinc-900 border-zinc-800 text-zinc-500 hover:text-zinc-300'}"
      title={isCoreRunning ? "内核监听中" : "内核未运行"}
      disabled
    >
      {isCoreRunning ? '●' : '○'}
    </button>
  </div>

  <!-- 下部：TUN / SYS 快捷操作 -->
  <div class="flex flex-col gap-2">
    <button
      onclick={toggleTun}
      disabled={guiState.isTunSwitchOn ? !guiState.canDisableTun : !guiState.canEnableTun}
      class="w-7 h-7 rounded-lg text-[10px] font-mono font-bold border transition-colors duration-150
             {tunUnconfirmed ? 'bg-amber-500/10 border-amber-500/30 text-amber-400' : isTunActive
               ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
               : isCoreRunning
                 ? 'bg-zinc-800 border-zinc-700/40 text-zinc-500 hover:text-zinc-300'
                 : 'bg-zinc-900 border-zinc-800 text-zinc-600'}"
      title={guiState.tunStatusError ? 'TUN 状态未知' : guiState.isTunDesiredEnabled && !guiState.isTunEnabled ? 'TUN 尚未运行；已保存开启设置，点击可取消' : isTunActive ? 'TUN 已开启' : 'TUN 未开启'}
    >
      {guiState.isSwitchingTun ? '…' : 'TUN'}
    </button>
    <button
      onclick={toggleSystemProxy}
      disabled={proxyEnabled ? !guiState.canDisableSystemProxy : !guiState.canEnableSystemProxy}
      class="w-7 h-7 rounded-lg text-[10px] font-mono font-bold border transition-colors duration-150
             {proxyEnabled
               ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
               : 'bg-zinc-800 border-zinc-700/40 text-zinc-500 hover:text-zinc-300'}"
      title={proxyEnabled ? "系统代理已开启" : "系统代理未开启"}
    >
      {guiState.isSwitchingSystemProxy ? '…' : 'SYS'}
    </button>
  </div>
</aside>
