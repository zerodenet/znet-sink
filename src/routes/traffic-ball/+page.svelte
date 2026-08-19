<script lang="ts">
  import { onMount } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { getAllWindows, getCurrentWindow } from '@tauri-apps/api/window';
  import { restoreMainWindow } from '$lib/services/traffic-ball';
  import type { CoreEventStatus, GuiEventPayload } from '$lib/types/core';

  const MIN_RATE_INTERVAL_MS = 500;
  const STALE_AFTER_MS = 2_500;

  let uploadBytesPerSecond = $state(0);
  let downloadBytesPerSecond = $state(0);
  let live = $state(false);
  let lastBytesUp: number | null = null;
  let lastBytesDown: number | null = null;
  let lastSampleAt = 0;
  let lastTrafficAt = 0;

  function numberFrom(value: unknown, keys: string[]): number | null {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
    const obj = value as Record<string, unknown>;
    for (const key of keys) {
      const candidate = obj[key];
      if (typeof candidate === 'number' && Number.isFinite(candidate)) return candidate;
    }
    return null;
  }

  function eventData(payload: GuiEventPayload): unknown {
    const eventPayload = payload.event?.payload;
    if (!eventPayload || typeof eventPayload !== 'object') return eventPayload;
    return 'data' in eventPayload ? eventPayload.data : eventPayload;
  }

  function applyTraffic(payload: GuiEventPayload) {
    if (document.visibilityState !== 'visible') return;
    if (payload.event?.eventType !== 'traffic.sampled') return;

    const data = eventData(payload);
    const bytesUp = numberFrom(data, ['bytesUp', 'bytes_up', 'upload', 'tx']);
    const bytesDown = numberFrom(data, ['bytesDown', 'bytes_down', 'download', 'rx']);
    if (bytesUp == null || bytesDown == null) return;

    const now = Date.now();
    if (lastBytesUp != null && lastBytesDown != null && lastSampleAt > 0) {
      const intervalMs = now - lastSampleAt;
      if (intervalMs >= MIN_RATE_INTERVAL_MS) {
        const upDelta = bytesUp >= lastBytesUp ? bytesUp - lastBytesUp : bytesUp;
        const downDelta = bytesDown >= lastBytesDown ? bytesDown - lastBytesDown : bytesDown;
        uploadBytesPerSecond = Math.max(0, upDelta * 1000 / intervalMs);
        downloadBytesPerSecond = Math.max(0, downDelta * 1000 / intervalMs);
      }
    }

    lastBytesUp = bytesUp;
    lastBytesDown = bytesDown;
    lastSampleAt = now;
    lastTrafficAt = now;
    live = true;
  }

  function formatRate(bytesPerSecond: number): string {
    if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) return '0 K/s';
    if (bytesPerSecond >= 1_000_000_000) return `${(bytesPerSecond / 1_000_000_000).toFixed(1)} G/s`;
    if (bytesPerSecond >= 1_000_000) return `${(bytesPerSecond / 1_000_000).toFixed(bytesPerSecond >= 10_000_000 ? 0 : 1)} M/s`;
    return `${Math.max(1, Math.round(bytesPerSecond / 1_000))} K/s`;
  }

  function formatFullRate(bytesPerSecond: number): string {
    if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) return '0 KB/s';
    if (bytesPerSecond >= 1_000_000_000) return `${(bytesPerSecond / 1_000_000_000).toFixed(2)} GB/s`;
    if (bytesPerSecond >= 1_000_000) return `${(bytesPerSecond / 1_000_000).toFixed(2)} MB/s`;
    return `${(bytesPerSecond / 1_000).toFixed(0)} KB/s`;
  }

  function handleMouseDown(event: MouseEvent) {
    // The second click of a double-click is reserved for restore, so only the
    // first click starts native dragging.
    if (event.button === 0 && event.detail === 1) {
      void getCurrentWindow().startDragging().catch(() => {});
    }
  }

  function restore() {
    void restoreMainWindow();
  }

  function handleContextMenu(event: MouseEvent) {
    event.preventDefault();
    restore();
  }

  async function reconcileWithMainWindow() {
    if (document.visibilityState !== 'visible') return;
    try {
      const windows = await getAllWindows();
      const main = windows.find((window) => window.label === 'main');
      if (main && await main.isVisible()) {
        await getCurrentWindow().hide();
      }
    } catch {
      // The tray/main-window path is best effort; traffic sampling remains valid.
    }
  }

  onMount(() => {
    let unlistenTraffic: UnlistenFn | null = null;
    let unlistenStatus: UnlistenFn | null = null;
    let unlistenExit: UnlistenFn | null = null;

    void listen<GuiEventPayload>('gui:event', (event) => applyTraffic(event.payload)).then((unlisten) => {
      unlistenTraffic = unlisten;
    });
    void listen<CoreEventStatus>('gui:event-status', (event) => {
      if (['offline', 'disconnected', 'error', 'stopped'].includes(event.payload.status)) {
        live = false;
        uploadBytesPerSecond = 0;
        downloadBytesPerSecond = 0;
      }
    }).then((unlisten) => {
      unlistenStatus = unlisten;
    });
    void listen('core:process-exited', () => {
      live = false;
      uploadBytesPerSecond = 0;
      downloadBytesPerSecond = 0;
    }).then((unlisten) => {
      unlistenExit = unlisten;
    });

    const staleTimer = window.setInterval(() => {
      if (lastTrafficAt > 0 && Date.now() - lastTrafficAt > STALE_AFTER_MS) {
        uploadBytesPerSecond = 0;
        downloadBytesPerSecond = 0;
      }
      void reconcileWithMainWindow();
    }, 1_000);

    return () => {
      window.clearInterval(staleTimer);
      unlistenTraffic?.();
      unlistenStatus?.();
      unlistenExit?.();
    };
  });
</script>

<svelte:head>
  <title>ZNet Sink Traffic</title>
</svelte:head>

<div
  class="traffic-ball"
  class:live
  onmousedown={handleMouseDown}
  ondblclick={restore}
  oncontextmenu={handleContextMenu}
  role="button"
  tabindex="0"
  aria-label={`实时流量，下载 ${formatFullRate(downloadBytesPerSecond)}，上传 ${formatFullRate(uploadBytesPerSecond)}。双击恢复主窗口。`}
  title={`下载 ${formatFullRate(downloadBytesPerSecond)} · 上传 ${formatFullRate(uploadBytesPerSecond)}\n拖动移动 · 双击或右键恢复主窗口`}
>
  <div class="traffic-ball-glow" aria-hidden="true"></div>
  <div class="traffic-ball-brand">
    <span class="traffic-ball-dot"></span>
    <span>ZNET</span>
  </div>
  <div class="traffic-rate traffic-rate-down">
    <svg viewBox="0 0 12 12" aria-hidden="true"><polyline points="2 5 6 9 10 5" /></svg>
    <strong>{formatRate(downloadBytesPerSecond)}</strong>
  </div>
  <div class="traffic-rate traffic-rate-up">
    <svg viewBox="0 0 12 12" aria-hidden="true"><polyline points="2 7 6 3 10 7" /></svg>
    <strong>{formatRate(uploadBytesPerSecond)}</strong>
  </div>
  <span class="traffic-ball-hint">双击恢复</span>
</div>

<style>
  :global(html),
  :global(body) {
    width: 100%;
    height: 100%;
    margin: 0;
    overflow: hidden;
    background: transparent !important;
    user-select: none;
  }

  :global(body) {
    display: grid;
    place-items: center;
  }

  .traffic-ball {
    position: relative;
    width: 104px;
    height: 104px;
    border-radius: 999px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    box-sizing: border-box;
    overflow: hidden;
    cursor: grab;
    color: rgba(255, 255, 255, 0.94);
    background:
      radial-gradient(circle at 36% 26%, rgba(255, 255, 255, 0.12), transparent 30%),
      linear-gradient(145deg, rgba(19, 27, 42, 0.97), rgba(9, 14, 24, 0.98));
    border: 1px solid rgba(148, 163, 184, 0.26);
    box-shadow:
      0 12px 30px rgba(0, 0, 0, 0.28),
      inset 0 1px 0 rgba(255, 255, 255, 0.08);
    font-family: var(--font-sans, system-ui, sans-serif);
    -webkit-font-smoothing: antialiased;
  }

  .traffic-ball:active { cursor: grabbing; }

  .traffic-ball-glow {
    position: absolute;
    inset: 5px;
    border-radius: inherit;
    border: 1px solid rgba(96, 165, 250, 0.12);
    pointer-events: none;
  }

  .traffic-ball.live .traffic-ball-glow {
    box-shadow: inset 0 0 18px rgba(59, 130, 246, 0.08);
  }

  .traffic-ball-brand {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-bottom: 2px;
    color: rgba(203, 213, 225, 0.72);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.12em;
  }

  .traffic-ball-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: rgba(148, 163, 184, 0.7);
    box-shadow: 0 0 0 3px rgba(148, 163, 184, 0.08);
  }

  .traffic-ball.live .traffic-ball-dot {
    background: #22c55e;
    box-shadow: 0 0 0 3px rgba(34, 197, 94, 0.12);
  }

  .traffic-rate {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 21px;
    font-variant-numeric: tabular-nums;
  }

  .traffic-rate svg {
    width: 11px;
    height: 11px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.7;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .traffic-rate strong {
    min-width: 52px;
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 13px;
    line-height: 1;
    font-weight: 700;
    letter-spacing: -0.035em;
  }

  .traffic-rate-down { color: #7dd3fc; }
  .traffic-rate-up { color: #86efac; }

  .traffic-ball-hint {
    margin-top: 2px;
    color: rgba(148, 163, 184, 0.58);
    font-size: 7px;
    line-height: 1;
    letter-spacing: 0.03em;
  }

  @media (prefers-reduced-motion: no-preference) {
    .traffic-ball.live .traffic-ball-dot {
      animation: live-pulse 2s ease-in-out infinite;
    }
  }

  @keyframes live-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.55; }
  }
</style>
