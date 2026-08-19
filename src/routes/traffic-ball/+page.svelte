<script lang="ts">
  import { onMount } from 'svelte';
  import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { getAllWindows, getCurrentWindow } from '@tauri-apps/api/window';
  import { getGuiTrafficStats } from '$lib/services/core';
  import {
    destroyTrafficBall,
    restoreMainWindow,
    snapTrafficBallToEdge,
    TRAFFIC_BALL_READY_EVENT,
  } from '$lib/services/traffic-ball';
  import type { CoreEventStatus } from '$lib/types/core';

  const MIN_RATE_INTERVAL_MS = 500;
  const STALE_AFTER_MS = 2_500;
  const EDGE_SNAP_DEBOUNCE_MS = 180;

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

  function applyTraffic(data: unknown) {
    if (document.visibilityState !== 'visible') return;

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
    if (bytesPerSecond >= 1_000_000) {
      return `${(bytesPerSecond / 1_000_000).toFixed(bytesPerSecond >= 10_000_000 ? 0 : 1)} M/s`;
    }
    return `${Math.max(1, Math.round(bytesPerSecond / 1_000))} K/s`;
  }

  function formatFullRate(bytesPerSecond: number): string {
    if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) return '0 KB/s';
    if (bytesPerSecond >= 1_000_000_000) return `${(bytesPerSecond / 1_000_000_000).toFixed(2)} GB/s`;
    if (bytesPerSecond >= 1_000_000) return `${(bytesPerSecond / 1_000_000).toFixed(2)} MB/s`;
    return `${(bytesPerSecond / 1_000).toFixed(0)} KB/s`;
  }

  function handleMouseDown(event: MouseEvent) {
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

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      restore();
    }
  }

  onMount(() => {
    let mounted = true;
    let unlistenTraffic: UnlistenFn | null = null;
    let unlistenStatus: UnlistenFn | null = null;
    let unlistenExit: UnlistenFn | null = null;
    let unlistenMainFocus: UnlistenFn | null = null;
    let unlistenMoved: UnlistenFn | null = null;
    let snapTimer: number | null = null;
    const ballWindow = getCurrentWindow();

    // Creation already requests an alpha-zero WebView. Repeat the setting once
    // the route is mounted so both creation-time and runtime rendering layers
    // agree before the main window swaps to this surface.
    void getCurrentWebview().setBackgroundColor([0, 0, 0, 0]).catch(() => {}).finally(() => {
      if (mounted) void emit(TRAFFIC_BALL_READY_EVENT);
    });

    void listen<Record<string, unknown>>('traffic.sampled', (event) => applyTraffic(event.payload)).then((unlisten) => {
      if (mounted) unlistenTraffic = unlisten;
      else unlisten();
    });

    void getGuiTrafficStats().then((stats) => {
      if (!mounted || lastTrafficAt > 0) return;
      const now = Date.now();
      uploadBytesPerSecond = Math.max(0, stats.uploadBytesPerSec);
      downloadBytesPerSecond = Math.max(0, stats.downloadBytesPerSec);
      lastBytesUp = stats.totalUploadBytes;
      lastBytesDown = stats.totalDownloadBytes;
      lastSampleAt = now;
      lastTrafficAt = now;
      live = true;
    }).catch(() => {});

    void listen<CoreEventStatus>('gui:event-status', (event) => {
      if (['offline', 'disconnected', 'error', 'stopped'].includes(event.payload.status)) {
        live = false;
        uploadBytesPerSecond = 0;
        downloadBytesPerSecond = 0;
      }
    }).then((unlisten) => {
      if (mounted) unlistenStatus = unlisten;
      else unlisten();
    });

    void listen('core:process-exited', () => {
      live = false;
      uploadBytesPerSecond = 0;
      downloadBytesPerSecond = 0;
    }).then((unlisten) => {
      if (mounted) unlistenExit = unlisten;
      else unlisten();
    });

    void getAllWindows().then(async (windows) => {
      const main = windows.find((window) => window.label === 'main');
      if (!main) return;
      const unlisten = await main.onFocusChanged(({ payload: focused }) => {
        if (focused) void destroyTrafficBall();
      });
      if (mounted) unlistenMainFocus = unlisten;
      else unlisten();
    }).catch(() => {});

    void ballWindow.onMoved(({ payload: position }) => {
      if (snapTimer != null) window.clearTimeout(snapTimer);
      snapTimer = window.setTimeout(() => {
        snapTimer = null;
        void snapTrafficBallToEdge(ballWindow, position);
      }, EDGE_SNAP_DEBOUNCE_MS);
    }).then((unlisten) => {
      if (mounted) unlistenMoved = unlisten;
      else unlisten();
    }).catch(() => {});

    const staleTimer = window.setInterval(() => {
      if (lastTrafficAt > 0 && Date.now() - lastTrafficAt > STALE_AFTER_MS) {
        live = false;
        uploadBytesPerSecond = 0;
        downloadBytesPerSecond = 0;
      }
    }, 1_000);

    return () => {
      mounted = false;
      window.clearInterval(staleTimer);
      if (snapTimer != null) window.clearTimeout(snapTimer);
      unlistenTraffic?.();
      unlistenStatus?.();
      unlistenExit?.();
      unlistenMainFocus?.();
      unlistenMoved?.();
    };
  });
</script>

<svelte:head>
  <title>ZNet Sink Traffic</title>
</svelte:head>

<button
  type="button"
  class="traffic-ball"
  class:live
  onmousedown={handleMouseDown}
  ondblclick={restore}
  oncontextmenu={handleContextMenu}
  onkeydown={handleKeyDown}
  aria-label={`实时流量，下载 ${formatFullRate(downloadBytesPerSecond)}，上传 ${formatFullRate(uploadBytesPerSecond)}。拖动可移动，双击恢复主窗口。`}
  title={`下载 ${formatFullRate(downloadBytesPerSecond)} · 上传 ${formatFullRate(uploadBytesPerSecond)}\n拖动移动 · 双击或右键恢复主窗口`}
>
  <span class="traffic-ball-highlight" aria-hidden="true"></span>
  <span class="traffic-ball-status" aria-hidden="true"></span>

  <span class="traffic-rate traffic-rate-down">
    <svg viewBox="0 0 12 12" aria-hidden="true"><polyline points="2 5 6 9 10 5" /></svg>
    <strong>{formatRate(downloadBytesPerSecond)}</strong>
  </span>
  <span class="traffic-divider" aria-hidden="true"></span>
  <span class="traffic-rate traffic-rate-up">
    <svg viewBox="0 0 12 12" aria-hidden="true"><polyline points="2 7 6 3 10 7" /></svg>
    <strong>{formatRate(uploadBytesPerSecond)}</strong>
  </span>
</button>

<style>
  :global(html),
  :global(body),
  :global(body > div) {
    width: 100%;
    height: 100%;
    margin: 0;
    padding: 0;
    overflow: hidden;
    background: transparent !important;
  }

  :global(html),
  :global(body) {
    user-select: none;
  }

  :global(body) {
    display: grid;
    place-items: center;
  }

  .traffic-ball {
    appearance: none;
    position: relative;
    isolation: isolate;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    padding: 0;
    margin: 0;
    border-radius: 50%;
    clip-path: circle(50% at 50% 50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    box-sizing: border-box;
    overflow: hidden;
    cursor: grab;
    color: rgba(255, 255, 255, 0.97);
    background:
      radial-gradient(circle at 31% 18%, rgba(255, 255, 255, 0.34), transparent 26%),
      radial-gradient(circle at 78% 83%, rgba(191, 219, 254, 0.24), transparent 43%),
      linear-gradient(150deg, #718096 0%, #5f728b 50%, #55657a 100%);
    border: 1px solid rgba(255, 255, 255, 0.28);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.40),
      inset 0 -14px 24px rgba(30, 41, 59, 0.14);
    font-family: var(--font-sans, system-ui, sans-serif);
    -webkit-font-smoothing: antialiased;
    transition: filter 0.16s ease, background 0.2s ease;
  }

  .traffic-ball.live {
    background:
      radial-gradient(circle at 31% 18%, rgba(255, 255, 255, 0.48), transparent 25%),
      radial-gradient(circle at 78% 83%, rgba(199, 210, 254, 0.30), transparent 44%),
      linear-gradient(150deg, #67b0f8 0%, #4389ee 47%, #5964df 100%);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.52),
      inset 0 -14px 24px rgba(30, 64, 175, 0.16);
  }

  .traffic-ball:hover {
    filter: brightness(1.045) saturate(1.03);
  }

  .traffic-ball:active {
    cursor: grabbing;
    filter: brightness(0.98);
  }

  .traffic-ball:focus-visible {
    outline: 2px solid rgba(224, 242, 254, 0.9);
    outline-offset: -4px;
  }

  .traffic-ball-highlight {
    position: absolute;
    inset: 4px;
    z-index: -1;
    border-radius: 50%;
    border: 1px solid rgba(255, 255, 255, 0.10);
    pointer-events: none;
  }

  .traffic-ball-status {
    position: absolute;
    top: 12px;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: rgba(226, 232, 240, 0.72);
    box-shadow: 0 0 0 2px rgba(255, 255, 255, 0.10);
  }

  .traffic-ball.live .traffic-ball-status {
    background: #86efac;
    box-shadow: 0 0 0 2px rgba(220, 252, 231, 0.18);
  }

  .traffic-rate {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    height: 17px;
    transform: translateY(3px);
    font-variant-numeric: tabular-nums;
    text-shadow: 0 1px 2px rgba(30, 64, 175, 0.18);
  }

  .traffic-rate svg {
    width: 10px;
    height: 10px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
    flex-shrink: 0;
  }

  .traffic-rate strong {
    width: 50px;
    text-align: left;
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 12px;
    line-height: 1;
    font-weight: 700;
    letter-spacing: -0.045em;
    white-space: nowrap;
  }

  .traffic-rate-down {
    color: #f0f9ff;
  }

  .traffic-rate-up {
    color: #ecfdf5;
  }

  .traffic-divider {
    width: 50px;
    height: 1px;
    transform: translateY(3px);
    background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.22), transparent);
  }

  @media (prefers-reduced-motion: no-preference) {
    .traffic-ball.live .traffic-ball-status {
      animation: live-pulse 2s ease-in-out infinite;
    }
  }

  @keyframes live-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.58; }
  }
</style>
