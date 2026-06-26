<script lang="ts">
  import { updater, formatBytes } from '$lib/services/updater.svelte';
  import { warning } from '$lib/services/toast.svelte';

  // Per-session dismissal — banner returns on next app start so users who
  // postpone still get reminded once per session without being nagged.
  let dismissed = $state(false);

  const visible = $derived(updater.updateAvailable && !dismissed);

  async function handleUpdate() {
    const ok = await updater.downloadAndInstall();
    if (!ok && updater.lastError) {
      warning(`更新失败: ${updater.lastError}`);
    }
  }
</script>

{#if visible}
  <div class="update-banner" role="status" aria-live="polite">
    <span class="update-dot pulse" aria-hidden="true"></span>
    <svg
      class="update-icon"
      width="15"
      height="15"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      stroke-width="1.6"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path d="M8 2v8M4.5 6.5L8 10l3.5-3.5M2.5 12.5h11" />
    </svg>
    <div class="update-content">
      {#if updater.downloading}
        <span class="update-label">下载更新中</span>
        <span class="update-progress-text">
          {updater.progressPct != null ? `${updater.progressPct}%` : '下载中'}
          <span class="update-bytes">
            · {formatBytes(updater.downloaded)}{updater.total != null ? ` / ${formatBytes(updater.total)}` : ''}
          </span>
        </span>
      {:else}
        <span class="update-label">发现新版本</span>
        <span class="update-version">v{updater.latestVersion}</span>
        <span class="update-current">（当前 v{updater.currentVersion}）</span>
      {/if}
    </div>
    <button class="update-action" onclick={handleUpdate} disabled={updater.downloading}>
      {updater.downloading ? '下载中…' : '立即更新'}
    </button>
    <button
      class="update-dismiss"
      onclick={() => (dismissed = true)}
      title="本次启动不再提示"
      aria-label="关闭更新提示"
    >
      <svg width="10" height="10" viewBox="0 0 10 10" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
        <line x1="2" y1="2" x2="8" y2="8" />
        <line x1="8" y1="2" x2="2" y2="8" />
      </svg>
    </button>
    {#if updater.downloading}
      <div class="update-progress-track" aria-hidden="true">
        <div
          class="update-progress-fill"
          class:indeterminate={updater.progressPct == null}
          style={updater.progressPct != null ? `width: ${updater.progressPct}%` : ''}
        ></div>
      </div>
    {/if}
  </div>
{/if}

<style>
  .update-banner {
    position: relative;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 9px 12px;
    border-radius: 10px;
    border: 1px solid rgba(245, 158, 11, 0.28);
    background: rgba(245, 158, 11, 0.08);
    flex-shrink: 0;
    overflow: hidden;
  }

  .update-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #f59e0b;
    flex-shrink: 0;
  }

  .update-dot.pulse {
    animation: update-pulse 1.4s ease-in-out infinite;
  }

  @keyframes update-pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.35;
    }
  }

  .update-icon {
    color: #d97706;
    flex-shrink: 0;
  }

  .update-content {
    display: flex;
    align-items: baseline;
    gap: 5px;
    flex: 1;
    min-width: 0;
    font-size: 12.5px;
  }

  .update-label {
    color: var(--foreground);
    font-weight: 600;
  }

  .update-version {
    font-family: var(--font-mono);
    font-weight: 600;
    color: #d97706;
    font-variant-numeric: tabular-nums;
  }

  .update-current {
    font-size: 11px;
    color: var(--muted-foreground);
    font-variant-numeric: tabular-nums;
  }

  .update-progress-text {
    font-size: 12px;
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    color: var(--foreground);
  }

  .update-bytes {
    color: var(--muted-foreground);
    font-size: 11px;
  }

  .update-progress-track {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 2px;
    background: rgba(245, 158, 11, 0.18);
    overflow: hidden;
  }

  .update-progress-fill {
    height: 100%;
    background: #f59e0b;
    transition: width 0.2s ease;
  }

  .update-progress-fill.indeterminate {
    width: 30%;
    animation: update-indeterminate 1.2s ease-in-out infinite;
  }

  @keyframes update-indeterminate {
    0% { transform: translateX(-100%); }
    100% { transform: translateX(400%); }
  }

  .update-action {
    border: none;
    border-radius: 6px;
    background: #f59e0b;
    color: #fff;
    font-size: 12px;
    font-weight: 600;
    padding: 5px 12px;
    cursor: pointer;
    transition: background 0.12s ease;
    flex-shrink: 0;
  }

  .update-action:hover:not(:disabled) {
    background: #d97706;
  }

  .update-action:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .update-dismiss {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--muted-foreground);
    cursor: pointer;
    flex-shrink: 0;
    transition:
      background 0.12s ease,
      color 0.12s ease;
  }

  .update-dismiss:hover {
    background: rgba(245, 158, 11, 0.14);
    color: var(--foreground);
  }

  :global(.dark) .update-banner {
    border-color: rgba(245, 158, 11, 0.32);
    background: rgba(245, 158, 11, 0.1);
  }
</style>
