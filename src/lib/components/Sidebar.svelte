<script lang="ts">
  import { guiState } from '$lib/services/gui-state.svelte';
  import {
    enableSystemProxy,
    disableSystemProxy,
    getGuiTunStatus,
  } from '$lib/services/core';
  import type { GuiFeatureStatus } from '$lib/types/gui-api';

  let tunStatus = $state<GuiFeatureStatus | null>(null);
  let proxyEnabled = $state(false);
  let loading = $state<{ tun: boolean; sys: boolean }>({ tun: false, sys: false });

  async function refreshTunStatus() {
    try {
      tunStatus = await getGuiTunStatus();
    } catch {
      tunStatus = null;
    }
  }

  // Sync proxy state from guiState connection data
  $effect(() => {
    proxyEnabled = guiState.connection?.systemProxyEnabled === true;
  });

  // Initial TUN status fetch when initialized
  $effect(() => {
    if (guiState.connection !== null) {
      refreshTunStatus();
    }
  });

  async function toggleSystemProxy() {
    if (loading.sys) return;
    loading.sys = true;
    try {
      if (proxyEnabled) {
        await disableSystemProxy();
        proxyEnabled = false;
      } else {
        await enableSystemProxy();
        proxyEnabled = true;
      }
    } catch {
      // Toggle failed, leave state unchanged
    } finally {
      loading.sys = false;
    }
  }

  async function toggleTun() {
    // TUN toggle requires backend support that doesn't exist yet
    // For now this shows status only
    await refreshTunStatus();
  }

  const isCoreRunning = $derived(guiState.connection?.state === 'connected');
  const isTunActive = $derived(tunStatus?.enabled === true);
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
      title={isCoreRunning ? "内核运行中" : "内核未运行"}
      disabled
    >
      {isCoreRunning ? '●' : '○'}
    </button>
  </div>

  <!-- 下部：TUN / SYS 快捷操作 -->
  <div class="flex flex-col gap-2">
    <button
      onclick={toggleTun}
      disabled={!isCoreRunning || loading.tun}
      class="w-7 h-7 rounded-lg text-[10px] font-mono font-bold border transition-colors duration-150
             {isTunActive
               ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
               : isCoreRunning
                 ? 'bg-zinc-800 border-zinc-700/40 text-zinc-500 hover:text-zinc-300'
                 : 'bg-zinc-900 border-zinc-800 text-zinc-600'}"
      title={isTunActive ? "TUN 已启用" : isCoreRunning ? "TUN 未启用 (点击刷新)" : "TUN 不可用 (内核未运行)"}
    >
      TUN
    </button>
    <button
      onclick={toggleSystemProxy}
      disabled={loading.sys}
      class="w-7 h-7 rounded-lg text-[10px] font-mono font-bold border transition-colors duration-150
             {proxyEnabled
               ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
               : 'bg-zinc-800 border-zinc-700/40 text-zinc-500 hover:text-zinc-300'}"
      title={proxyEnabled ? "系统代理已开启 (点击关闭)" : "系统代理已关闭 (点击开启)"}
    >
      {loading.sys ? '…' : 'SYS'}
    </button>
  </div>
</aside>
