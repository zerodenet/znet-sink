<script lang="ts">
  import { onMount } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { getAllWindows, getCurrentWindow } from '@tauri-apps/api/window';
  import { getGuiTrafficStats } from '$lib/services/core';
  import {
    hideTrafficBall,
    restoreMainWindow,
    snapTrafficBallToEdge,
    TRAFFIC_BALL_HIDDEN_EVENT,
    TRAFFIC_BALL_SHOWN_EVENT,
  } from '$lib/services/traffic-ball';
  import type { CoreEventStatus } from '$lib/types/core';

  const MIN_RATE_INTERVAL_MS = 500;
  const STALE_AFTER_MS = 2_500;
  const EDGE_SNAP_DEBOUNCE_MS = 180;
  const INNER_TOP = 4.5;
  const INNER_BOTTOM = 91.5;
  const MIN_LIQUID_FILL = 0.18;
  const MAX_LIQUID_FILL = 0.72;

  let uploadBytesPerSecond = $state(0);
  let downloadBytesPerSecond = $state(0);
  let live = $state(false);
  let surfaceVisible = false;
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

  function resetTraffic() {
    uploadBytesPerSecond = 0;
    downloadBytesPerSecond = 0;
    live = false;
    lastBytesUp = null;
    lastBytesDown = null;
    lastSampleAt = 0;
    lastTrafficAt = 0;
  }

  async function seedTrafficSnapshot() {
    try {
      const stats = await getGuiTrafficStats();
      if (!surfaceVisible) return;
      const now = Date.now();
      uploadBytesPerSecond = Math.max(0, stats.uploadBytesPerSec);
      downloadBytesPerSecond = Math.max(0, stats.downloadBytesPerSec);
      lastBytesUp = stats.totalUploadBytes;
      lastBytesDown = stats.totalDownloadBytes;
      lastSampleAt = now;
      lastTrafficAt = now;
      live = true;
    } catch {
      if (surfaceVisible) resetTraffic();
    }
  }

  function applyTraffic(data: unknown) {
    if (!surfaceVisible) return;

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

  function totalRate(): number {
    return Math.max(0, uploadBytesPerSecond) + Math.max(0, downloadBytesPerSecond);
  }

  // The liquid is an activity indicator, not a bandwidth-capacity meter.
  // A logarithmic scale keeps common desktop rates visually useful without
  // making a brief high-speed transfer pin the surface to the top forever.
  function trafficActivity(): number {
    const bytesPerSecond = totalRate();
    if (bytesPerSecond <= 0) return 0;
    return Math.max(0, Math.min(1, (Math.log10(bytesPerSecond) - 3) / 5));
  }

  function liquidSurfaceY(): number {
    const fill = MIN_LIQUID_FILL
      + (MAX_LIQUID_FILL - MIN_LIQUID_FILL) * trafficActivity();
    return INNER_BOTTOM - (INNER_BOTTOM - INNER_TOP) * fill;
  }

  function waveDuration(): number {
    return 5.6 - 3.3 * trafficActivity();
  }

  function formatRate(bytesPerSecond: number): string {
    if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) return '0 KB/s';
    if (bytesPerSecond >= 1_000_000_000) {
      return `${(bytesPerSecond / 1_000_000_000).toFixed(bytesPerSecond >= 10_000_000_000 ? 0 : 1)} GB/s`;
    }
    if (bytesPerSecond >= 1_000_000) {
      return `${(bytesPerSecond / 1_000_000).toFixed(bytesPerSecond >= 10_000_000 ? 0 : 1)} MB/s`;
    }
    if (bytesPerSecond >= 1_000) {
      return `${Math.max(1, Math.round(bytesPerSecond / 1_000))} KB/s`;
    }
    return `${Math.max(1, Math.round(bytesPerSecond))} B/s`;
  }

  function formatFullRate(bytesPerSecond: number): string {
    if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) return '0 KB/s';
    if (bytesPerSecond >= 1_000_000_000) return `${(bytesPerSecond / 1_000_000_000).toFixed(2)} GB/s`;
    if (bytesPerSecond >= 1_000_000) return `${(bytesPerSecond / 1_000_000).toFixed(2)} MB/s`;
    if (bytesPerSecond >= 1_000) return `${(bytesPerSecond / 1_000).toFixed(1)} KB/s`;
    return `${Math.round(bytesPerSecond)} B/s`;
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
    let unlistenShown: UnlistenFn | null = null;
    let unlistenHidden: UnlistenFn | null = null;
    let unlistenTraffic: UnlistenFn | null = null;
    let unlistenStatus: UnlistenFn | null = null;
    let unlistenExit: UnlistenFn | null = null;
    let unlistenMainFocus: UnlistenFn | null = null;
    let unlistenMoved: UnlistenFn | null = null;
    let snapTimer: number | null = null;
    const ballWindow = getCurrentWindow();

    void listen(TRAFFIC_BALL_SHOWN_EVENT, () => {
      surfaceVisible = true;
      resetTraffic();
      surfaceVisible = true;
      void seedTrafficSnapshot();
    }).then((unlisten) => {
      if (mounted) unlistenShown = unlisten;
      else unlisten();
    });

    void listen(TRAFFIC_BALL_HIDDEN_EVENT, () => {
      surfaceVisible = false;
      resetTraffic();
    }).then((unlisten) => {
      if (mounted) unlistenHidden = unlisten;
      else unlisten();
    });

    void ballWindow.isVisible().then((visible) => {
      if (!mounted || !visible) return;
      surfaceVisible = true;
      void seedTrafficSnapshot();
    }).catch(() => {});

    void listen<Record<string, unknown>>('traffic.sampled', (event) => applyTraffic(event.payload)).then((unlisten) => {
      if (mounted) unlistenTraffic = unlisten;
      else unlisten();
    });

    void listen<CoreEventStatus>('gui:event-status', (event) => {
      if (['offline', 'disconnected', 'error', 'stopped'].includes(event.payload.status)) {
        uploadBytesPerSecond = 0;
        downloadBytesPerSecond = 0;
        live = false;
      }
    }).then((unlisten) => {
      if (mounted) unlistenStatus = unlisten;
      else unlisten();
    });

    void listen('core:process-exited', () => {
      uploadBytesPerSecond = 0;
      downloadBytesPerSecond = 0;
      live = false;
    }).then((unlisten) => {
      if (mounted) unlistenExit = unlisten;
      else unlisten();
    });

    // The main window remains the tray/menu authority. If it is restored by
    // any native path, simply hide this already-created transparent surface.
    void getAllWindows().then(async (windows) => {
      const main = windows.find((window) => window.label === 'main');
      if (!main) return;
      const unlisten = await main.onFocusChanged(({ payload: focused }) => {
        if (focused) void hideTrafficBall();
      });
      if (mounted) unlistenMainFocus = unlisten;
      else unlisten();
    }).catch(() => {});

    void ballWindow.onMoved(({ payload: position }) => {
      if (!surfaceVisible) return;
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
      if (surfaceVisible && lastTrafficAt > 0 && Date.now() - lastTrafficAt > STALE_AFTER_MS) {
        uploadBytesPerSecond = 0;
        downloadBytesPerSecond = 0;
        live = false;
      }
    }, 1_000);

    return () => {
      mounted = false;
      window.clearInterval(staleTimer);
      if (snapTimer != null) window.clearTimeout(snapTimer);
      unlistenShown?.();
      unlistenHidden?.();
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
>
  <svg
    class="traffic-fluid"
    viewBox="0 0 96 96"
    aria-hidden="true"
    style={`--liquid-y: ${liquidSurfaceY()}px; --wave-duration: ${waveDuration()}s; --wave-duration-back: ${waveDuration() * 1.24}s;`}
  >
    <defs>
      <clipPath id="traffic-ball-liquid-clip">
        <circle cx="48" cy="48" r="43.2" />
      </clipPath>
      <linearGradient id="traffic-ball-shell" x1="18" y1="11" x2="78" y2="87" gradientUnits="userSpaceOnUse">
        <stop offset="0" stop-color="#b9d7f2" stop-opacity="0.88" />
        <stop offset="0.45" stop-color="#6f91b6" stop-opacity="0.92" />
        <stop offset="1" stop-color="#455c78" stop-opacity="0.95" />
      </linearGradient>
      <linearGradient id="traffic-ball-liquid-back" x1="22" y1="0" x2="77" y2="86" gradientUnits="userSpaceOnUse">
        <stop offset="0" stop-color="#7dd3fc" stop-opacity="0.72" />
        <stop offset="1" stop-color="#818cf8" stop-opacity="0.68" />
      </linearGradient>
      <linearGradient id="traffic-ball-liquid-front" x1="19" y1="2" x2="79" y2="89" gradientUnits="userSpaceOnUse">
        <stop offset="0" stop-color="#38bdf8" stop-opacity="0.82" />
        <stop offset="0.52" stop-color="#3b82f6" stop-opacity="0.86" />
        <stop offset="1" stop-color="#6366f1" stop-opacity="0.88" />
      </linearGradient>
    </defs>

    <circle class="glass-shell" cx="48" cy="48" r="44" fill="url(#traffic-ball-shell)" />

    <g clip-path="url(#traffic-ball-liquid-clip)">
      <g class="liquid-layer">
        <path
          class="wave wave-back"
          fill="url(#traffic-ball-liquid-back)"
          d="M -96 1 Q -72 4 -48 1 T 0 1 T 48 1 T 96 1 T 144 1 T 192 1 V 100 H -96 Z"
        />
        <path
          class="wave wave-front"
          fill="url(#traffic-ball-liquid-front)"
          d="M -96 0 Q -72 -4 -48 0 T 0 0 T 48 0 T 96 0 T 144 0 T 192 0 V 100 H -96 Z"
        />
      </g>

      <ellipse
        class="glass-reflection"
        cx="33"
        cy="25"
        rx="16"
        ry="7"
        transform="rotate(-24 33 25)"
      />
    </g>

    <circle class="inner-rim" cx="48" cy="48" r="42.6" />
    <circle class="outer-rim" cx="48" cy="48" r="44" />
  </svg>

  <span class="traffic-readout">
    <span class="traffic-rate traffic-rate-down">
      <svg viewBox="0 0 12 12" aria-hidden="true"><polyline points="2 5 6 9 10 5" /></svg>
      <strong>{formatRate(downloadBytesPerSecond)}</strong>
    </span>
    <span class="traffic-divider" aria-hidden="true"></span>
    <span class="traffic-rate traffic-rate-up">
      <svg viewBox="0 0 12 12" aria-hidden="true"><polyline points="2 7 6 3 10 7" /></svg>
      <strong>{formatRate(uploadBytesPerSecond)}</strong>
    </span>
  </span>

  <span class="traffic-hint" aria-hidden="true">
    <span class="traffic-hint-primary">
      <svg viewBox="0 0 14 14"><rect x="2.25" y="2.25" width="9.5" height="9.5" rx="2" /><path d="M5 7h4M7 5v4" /></svg>
      <strong>双击打开</strong>
    </span>
    <span class="traffic-hint-divider"></span>
    <span class="traffic-hint-secondary">拖动移动</span>
  </span>
</button>

<style>
  :global(html),
  :global(body),
  :global(body > div) {
    width: 100vw;
    height: 100vh;
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
    width: min(100vw, 100vh);
    height: min(100vw, 100vh);
    aspect-ratio: 1 / 1;
    flex: 0 0 auto;
    min-width: 0;
    min-height: 0;
    padding: 0;
    margin: 0;
    border: 0;
    outline: 0;
    overflow: visible;
    cursor: grab;
    color: rgba(255, 255, 255, 0.98);
    background: transparent;
    font-family: var(--font-sans, system-ui, sans-serif);
    -webkit-font-smoothing: antialiased;
    transition: filter 0.16s ease, transform 0.16s ease;
  }

  .traffic-fluid {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
    overflow: visible;
    pointer-events: none;
  }

  .glass-shell {
    transition: opacity 0.24s ease;
  }

  .liquid-layer {
    transform: translateY(var(--liquid-y));
    transform-box: view-box;
    transform-origin: 0 0;
    transition: transform 680ms cubic-bezier(0.22, 1, 0.36, 1), opacity 0.24s ease;
  }

  .wave {
    transform-box: view-box;
    transform-origin: 0 0;
    will-change: transform;
  }

  .wave-back {
    opacity: 0.68;
  }

  .wave-front {
    opacity: 0.9;
  }

  .glass-reflection {
    fill: rgba(255, 255, 255, 0.16);
    pointer-events: none;
  }

  .inner-rim {
    fill: none;
    stroke: rgba(255, 255, 255, 0.10);
    stroke-width: 0.7;
    pointer-events: none;
  }

  .outer-rim {
    fill: none;
    stroke: rgba(225, 239, 255, 0.60);
    stroke-width: 0.9;
    pointer-events: none;
  }

  .traffic-readout {
    position: absolute;
    inset: 0;
    z-index: 2;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    pointer-events: none;
    opacity: 1;
    transform: translateY(1px) scale(1);
    transition: opacity 140ms ease, transform 180ms ease;
  }

  .traffic-rate {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    height: 16px;
    font-variant-numeric: tabular-nums;
    text-shadow: 0 1px 2px rgba(15, 23, 42, 0.42);
  }

  .traffic-rate svg {
    width: 9px;
    height: 9px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
    flex-shrink: 0;
  }

  .traffic-rate strong {
    width: 57px;
    text-align: left;
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 10.5px;
    line-height: 1;
    font-weight: 680;
    letter-spacing: -0.045em;
    white-space: nowrap;
  }

  .traffic-rate-down {
    color: #f5fbff;
  }

  .traffic-rate-up {
    color: #f0fdf9;
  }

  .traffic-divider {
    width: 40px;
    height: 1px;
    background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.20), transparent);
  }

  .traffic-hint {
    position: absolute;
    inset: 11px;
    z-index: 3;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 5px;
    border-radius: 50%;
    color: rgba(248, 252, 255, 0.98);
    pointer-events: none;
    opacity: 0;
    transform: scale(0.93);
    text-shadow: 0 1px 2px rgba(15, 23, 42, 0.46);
    transition: opacity 140ms ease, transform 170ms cubic-bezier(0.22, 1, 0.36, 1);
  }

  .traffic-hint-primary {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    line-height: 1;
    white-space: nowrap;
  }

  .traffic-hint-primary svg {
    width: 10px;
    height: 10px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.25;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .traffic-hint-primary strong {
    font-weight: 650;
  }

  .traffic-hint-divider {
    width: 34px;
    height: 1px;
    background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.28), transparent);
  }

  .traffic-hint-secondary {
    font-size: 8.5px;
    line-height: 1;
    color: rgba(239, 246, 255, 0.76);
    white-space: nowrap;
  }

  .traffic-ball:not(.live) .glass-shell {
    opacity: 0.76;
  }

  .traffic-ball:not(.live) .liquid-layer {
    opacity: 0.24;
  }

  .traffic-ball:not(.live) .outer-rim {
    stroke: rgba(226, 232, 240, 0.38);
  }

  .traffic-ball:hover {
    filter: brightness(1.04) saturate(1.035);
  }

  .traffic-ball:hover .traffic-readout,
  .traffic-ball:focus-visible .traffic-readout {
    opacity: 0.08;
    transform: translateY(1px) scale(0.96);
    transition-delay: 180ms;
  }

  .traffic-ball:hover .traffic-hint,
  .traffic-ball:focus-visible .traffic-hint {
    opacity: 1;
    transform: scale(1);
    transition-delay: 220ms;
  }

  .traffic-ball:active {
    cursor: grabbing;
    transform: scale(0.985);
  }

  .traffic-ball:active .traffic-hint {
    opacity: 0;
    transform: scale(0.96);
    transition-delay: 0ms;
  }

  .traffic-ball:active .traffic-readout {
    opacity: 1;
    transition-delay: 0ms;
  }

  .traffic-ball:focus-visible::after {
    content: '';
    position: absolute;
    inset: 5px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 1.5px rgba(224, 242, 254, 0.92);
    pointer-events: none;
  }

  @media (prefers-reduced-motion: no-preference) {
    .traffic-ball.live .wave-front {
      animation: traffic-wave-forward var(--wave-duration) linear infinite;
    }

    .traffic-ball.live .wave-back {
      animation: traffic-wave-back var(--wave-duration-back) linear infinite;
    }

    .traffic-ball.live .glass-reflection {
      animation: glass-breathe 4.8s ease-in-out infinite;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .liquid-layer,
    .traffic-readout,
    .traffic-hint {
      transition: none;
    }
  }

  @keyframes traffic-wave-forward {
    from { transform: translateX(0); }
    to { transform: translateX(-96px); }
  }

  @keyframes traffic-wave-back {
    from { transform: translateX(-96px); }
    to { transform: translateX(0); }
  }

  @keyframes glass-breathe {
    0%, 100% { opacity: 0.68; }
    50% { opacity: 1; }
  }
</style>
