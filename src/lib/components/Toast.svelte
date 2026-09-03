<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { store } from '$lib/services/store.svelte';
  import { getToasts, dismissToast, type ToastType } from '$lib/services/toast.svelte';
  import { MAX_ACTIVE_TOASTS } from '$lib/services/toast-policy';

  const toasts = getToasts();
  const visibleToasts = $derived(Array.from(toasts.values()).slice(-MAX_ACTIVE_TOASTS));

  function getAccentColor(type: ToastType): string {
    switch (type) {
      case 'success': return 'var(--success)';
      case 'error': return 'var(--destructive)';
      case 'warning': return 'var(--warning)';
      case 'info': return 'var(--accent-foreground)';
    }
  }

  function getIconBg(type: ToastType): string {
    switch (type) {
      case 'success': return 'rgba(34,197,94,0.12)';
      case 'error': return 'rgba(239,68,68,0.12)';
      case 'warning': return 'rgba(245,158,11,0.12)';
      case 'info': return 'rgba(99,102,241,0.12)';
    }
  }

  function getLabel(type: ToastType): string {
    switch (type) {
      case 'success': return '成功';
      case 'error': return '错误';
      case 'warning': return '警告';
      case 'info': return '提示';
    }
  }

  function openLogs(id: number) {
    dismissToast(id);
    store.isInitialized = true;
    store.activeTab = 'logs';
  }
</script>

{#if visibleToasts.length > 0}
  <div class="toast-container" aria-live="polite" aria-label="应用提示">
    {#each visibleToasts as toast (toast.id)}
      <div
        class="toast-item"
        class:error={toast.type === 'error'}
        style="--accent: {getAccentColor(toast.type)};"
        role={toast.type === 'error' ? 'alert' : 'status'}
      >
        <div class="toast-bar" style="background: var(--accent);"></div>

        <div
          class="toast-icon"
          style="background: {getIconBg(toast.type)}; color: var(--accent);"
          aria-hidden="true"
        >
          {#if toast.type === 'warning'}
            <svg width="13" height="13" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
              <line x1="6" y1="2.5" x2="6" y2="7"/>
              <circle cx="6" cy="9.5" r="0.6" fill="currentColor" stroke="none"/>
            </svg>
          {:else if toast.type === 'info'}
            <svg width="13" height="13" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
              <circle cx="6" cy="2.5" r="0.6" fill="currentColor" stroke="none"/>
              <line x1="6" y1="4.5" x2="6" y2="9.5"/>
            </svg>
          {:else if toast.type === 'error'}
            <svg width="13" height="13" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
              <line x1="2.5" y1="2.5" x2="9.5" y2="9.5"/>
              <line x1="9.5" y1="2.5" x2="2.5" y2="9.5"/>
            </svg>
          {:else}
            <svg width="13" height="13" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="2,6 5,9 10,3"/>
            </svg>
          {/if}
        </div>

        <div class="toast-text">
          <div class="toast-heading">
            <span class="toast-label" style="color: var(--accent);">{getLabel(toast.type)}</span>
            <span class="toast-recorded">已记录到应用日志</span>
          </div>
          <span class="toast-msg">{toast.message}</span>
        </div>

        <Button variant="outline" size="sm"  type="button" onclick={() => openLogs(toast.id)}>
          查看日志
        </Button>
        <Button variant="ghost" size="icon-sm"
          onclick={() => dismissToast(toast.id)}

          type="button"
          aria-label="关闭提示"
        >
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
            <line x1="2" y1="2" x2="8" y2="8"/>
            <line x1="8" y1="2" x2="2" y2="8"/>
          </svg>
        </Button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .toast-container {
    position: absolute;
    top: calc(100% + 7px);
    left: 50%;
    z-index: 9999;
    width: min(660px, calc(100vw - 32px));
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    gap: 7px;
    pointer-events: none;
  }

  .toast-item {
    pointer-events: auto;
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 48px;
    padding: 8px 9px 8px 15px;
    background: color-mix(in srgb, var(--card) 94%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 24%, var(--border));
    border-radius: 9px;
    overflow: hidden;
    box-shadow:
      0 10px 32px rgba(0, 0, 0, 0.16),
      0 2px 8px rgba(0, 0, 0, 0.08);
    animation: toast-in 0.2s cubic-bezier(0.22, 1, 0.36, 1);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
  }

  .toast-item.error {
    box-shadow:
      0 10px 34px rgba(127, 29, 29, 0.2),
      0 2px 8px rgba(0, 0, 0, 0.1);
  }

  :global(.dark) .toast-item {
    box-shadow:
      0 12px 36px rgba(0, 0, 0, 0.5),
      0 2px 8px rgba(0, 0, 0, 0.32);
  }

  .toast-bar {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 4px;
  }

  .toast-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 27px;
    height: 27px;
    border-radius: 7px;
    flex-shrink: 0;
  }

  .toast-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .toast-heading {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
  }

  .toast-label {
    font-size: 12px;
    font-weight: 700;
    line-height: 1;
  }

  .toast-recorded {
    color: var(--muted-foreground);
    font-size: 10px;
    line-height: 1;
    opacity: 0.7;
  }

  .toast-msg {
    max-height: 4.2em;
    overflow: auto;
    color: var(--foreground);
    font-size: 12px;
    font-weight: 450;
    line-height: 1.4;
    overflow-wrap: anywhere;
    user-select: text;
    white-space: normal;
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(-8px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  @media (max-width: 640px) {
    .toast-container {
      width: calc(100vw - 20px);
    }

    .toast-recorded {
      display: none;
    }
  }
</style>
