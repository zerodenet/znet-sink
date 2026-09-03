<script lang="ts">
  import { guiState } from '$lib/services/gui-state.svelte';
  import { store } from '$lib/services/store.svelte';

  // Derived connection / process state
  const c = $derived(guiState.connection);
  const isCoreAvailable = $derived(c?.coreAvailable === true || c?.processState === 'running');
  const isProcessRunning = $derived(c?.processState === 'running');
  const isProcessStarting = $derived(c?.processState === 'starting');
  const isProcessFailed = $derived(c?.processState === 'failed');
  const isCrashed = $derived(c?.processExitReason === 'crashed');
  const isSystemProxyEnabled = $derived(c?.systemProxyEnabled === true);
  const isBusy = $derived(
    guiState.isConnecting
      || guiState.isDisconnecting
      || guiState.isStartingCore
      || guiState.isStoppingCore
      || guiState.isSwitchingMode,
  );

  type Tone = 'on' | 'listening' | 'busy' | 'error' | 'off';
  type Status = { tone: Tone; label: string; title: string };

  const status = $derived.by<Status>(() => {
    if (guiState.isInitializing) {
      return { tone: 'busy', label: '初始化', title: '正在加载应用状态' };
    }
    if (isSystemProxyEnabled) {
      return { tone: 'on', label: '服务中', title: '系统代理已开启，内核运行中' };
    }
    if (isBusy) {
      return {
        tone: 'busy',
        label: guiState.isConnecting ? '启用中' :
               guiState.isDisconnecting ? '关闭中' :
               guiState.isStartingCore ? '启动中' :
               guiState.isStoppingCore ? '重启中' : '切换中',
        title: '正在切换状态…',
      };
    }
    if (isProcessFailed || isCrashed) {
      return { tone: 'error', label: '异常', title: c?.message ?? '内核异常退出' };
    }
    if (isCoreAvailable || isProcessStarting) {
      return { tone: 'listening', label: '内核监听', title: '内核运行中，系统代理未开启' };
    }
    return { tone: 'off', label: '已停止', title: '内核未运行' };
  });

  const dotColor = $derived(
    status.tone === 'on' ? '#22C55E' :
    status.tone === 'listening' ? '#F59E0B' :
    status.tone === 'busy' ? '#F59E0B' :
    status.tone === 'error' ? '#EF4444' :
    'var(--muted-foreground)',
  );

  function handleClick() {
    // Jump to overview so the user can see full details / take action.
    store.activeTab = 'overview';
  }
</script>

<button data-slot="surface-button"
  class="kernel-pill tone-{status.tone}"
  onclick={handleClick}
  title={status.title}
  aria-label="内核状态：{status.label}"
>
  <span class="pill-dot" class:pulse={isBusy || status.tone === 'listening'} style="background: {dotColor};"></span>
  <span class="pill-label">{status.label}</span>
</button>

<style>
  .kernel-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 9px;
    border-radius: 6px;
    border: 1px solid var(--titlebar-border, var(--border));
    background: transparent;
    cursor: pointer;
    transition: background 0.13s ease, border-color 0.13s ease;
    flex-shrink: 0;
  }

  .kernel-pill:hover {
    background: var(--muted);
    border-color: var(--border);
  }

  .pill-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
    transition: background 0.2s ease;
  }

  .pill-dot.pulse {
    animation: pulse-dot 1.4s ease-in-out infinite;
  }

  @keyframes pulse-dot {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }

  .pill-label {
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.01em;
    color: var(--foreground);
    white-space: nowrap;
    line-height: 1;
  }

  /* Tone-specific accents on the label color */
  .kernel-pill.tone-on .pill-label { color: #16A34A; }
  .kernel-pill.tone-listening .pill-label,
  .kernel-pill.tone-busy .pill-label { color: #D97706; }
  .kernel-pill.tone-error .pill-label { color: var(--destructive); }
  .kernel-pill.tone-off .pill-label { color: var(--muted-foreground); }

  :global(.dark) .kernel-pill.tone-on .pill-label { color: #4ADE80; }
  :global(.dark) .kernel-pill.tone-listening .pill-label,
  :global(.dark) .kernel-pill.tone-busy .pill-label { color: #FBBF24; }
</style>
