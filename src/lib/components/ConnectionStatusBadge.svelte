<script lang="ts">
  import { guiState } from '$lib/services/gui-state.svelte';

  const isCoreRunning = $derived(guiState.connection?.state === 'connected');
  const isCoreStarting = $derived(guiState.connection?.state === 'connecting');
  const isProxyEnabled = $derived(guiState.connection?.systemProxyEnabled === true);

  type Status = 'off' | 'core-only' | 'proxy-active' | 'error';
  const status: Status = $derived(
    guiState.connection?.state === 'error' ? 'error' :
    isProxyEnabled && isCoreRunning ? 'proxy-active' :
    isCoreRunning || isCoreStarting ? 'core-only' :
    'off'
  );

  const dotColor = $derived(
    status === 'proxy-active' ? '#22C55E' :
    status === 'core-only'   ? '#F59E0B' :
    status === 'error'       ? '#EF4444' :
    'var(--muted-foreground)'
  );

  const label = $derived(
    status === 'proxy-active' ? '运行中' :
    status === 'core-only'   ? '内核运行' :
    status === 'error'       ? '异常' :
    '未激活'
  );
</script>

<div class="status-badge" class:error={status === 'error'} class:active={status === 'proxy-active'}>
  <span
    class="status-dot"
    class:pulse={status === 'error' || status === 'core-only'}
    style="background: {dotColor};"
  ></span>
  <span class="status-label">{label}</span>
</div>

<style>
  .status-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 9px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--surface, var(--card));
  }

  .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .status-dot.pulse {
    animation: pulse-dot 1.8s ease-in-out infinite;
  }

  .status-label {
    font-size: 11.5px;
    font-weight: 500;
    color: var(--muted-foreground);
    letter-spacing: 0.01em;
    white-space: nowrap;
  }

  .status-badge.error .status-label {
    color: var(--destructive);
  }

  .status-badge.active .status-label {
    color: var(--success);
  }

  @keyframes pulse-dot {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.35; }
  }
</style>
