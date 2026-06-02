<script lang="ts">
  import { detectKernelVersion, listKernelVersions } from '$lib/services/kernel-version';
  import { getGuiCoreHealth } from '$lib/services/core';
  import type { KernelRelease } from '$lib/types/kernel-version';
  import { store } from '$lib/services/store.svelte';

  let currentVersion = $state<string | null>(null);
  let latestStable = $state<KernelRelease | null>(null);
  let updateAvailable = $state(false);
  let checking = $state(false);
  let mounted = $state(false);

  /** Strip leading 'v' so all version comparisons are prefix-free. */
  function stripV(v: string): string {
    return v.startsWith('v') ? v.slice(1) : v;
  }

  const hasVersion = $derived(currentVersion !== null);
  const latestVersion = $derived(latestStable?.version ?? null);

  async function checkVersion() {
    checking = true;
    try {
      // Detect current installed version
      const detect = await detectKernelVersion();
      currentVersion = detect.version ? stripV(detect.version) : null;

      // Also check running version (may be newer)
      try {
        const health = await getGuiCoreHealth();
        if (health.engineVersion) {
          currentVersion = stripV(health.engineVersion);
        }
      } catch { /* health API may be unavailable if core not running */ }

      // Fetch latest stable from GitHub
      const list = await listKernelVersions();
      const stable = list.versions
        .filter(v => v.channel === 'stable')
        .sort((a, b) => (b.publishedAtUnixMs ?? 0) - (a.publishedAtUnixMs ?? 0));

      if (stable.length > 0) {
        const top = stable[0];
        latestStable = top;
        if (currentVersion && top.version !== currentVersion) {
          updateAvailable = true;
        }
      }
    } catch {
      // Network may not be available — silent fallback
    } finally {
      checking = false;
    }
  }

  $effect(() => {
    if (store.isInitialized && !mounted) {
      mounted = true;
      checkVersion();
    }
  });

  function openKernelSettings() {
    store.openSettings('core');
  }
</script>

<div class="kernel-card">
  <div class="kernel-header">
    <span class="kernel-label">内核版本</span>
    {#if checking}
      <span class="kernel-state muted">检查中…</span>
    {:else if hasVersion}
      <span class="kernel-state">
        <span class="kernel-dot" class:upgrade={updateAvailable}></span>
        v{currentVersion}
      </span>
    {:else}
      <span class="kernel-state muted">未安装</span>
    {/if}
  </div>

  {#if hasVersion}
    <div class="kernel-meta">
      {#if updateAvailable && latestVersion}
        <div class="update-avail">
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <path d="M5 1.5v5M2.5 4L5 6.5 7.5 4"/>
            <line x1="1" y1="9" x2="9" y2="9"/>
          </svg>
          <span class="update-text">v{latestVersion} 可用</span>
        </div>
      {:else}
        <div class="up-to-date">已是最新</div>
      {/if}
    </div>
  {/if}

  <button class="kernel-link" onclick={openKernelSettings}>
    {hasVersion ? '管理版本' : '安装内核'}
  </button>
</div>

<style>
  .kernel-card {
    display: flex;
    flex-direction: column;
    gap: 7px;
    min-height: 96px;
    padding: 11px 13px;
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
    overflow: hidden;
    transition: box-shadow 0.15s ease, transform 0.15s ease;
  }

  .kernel-card:hover {
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.07);
    transform: translateY(-0.5px);
  }

  :global(.dark) .kernel-card { box-shadow: 0 1px 3px rgba(0, 0, 0, 0.22); }
  :global(.dark) .kernel-card:hover { box-shadow: 0 2px 8px rgba(0, 0, 0, 0.32); }

  .kernel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
  }

  .kernel-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--muted-foreground);
  }

  .kernel-state {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--foreground);
    font-variant-numeric: tabular-nums;
  }

  .kernel-state.muted {
    color: var(--muted-foreground);
    font-weight: 500;
  }

  .kernel-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
    background: #22C55E;
  }

  .kernel-dot.upgrade {
    background: #F59E0B;
    animation: pulse-dot 1.4s ease-in-out infinite;
  }

  @keyframes pulse-dot {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.3; }
  }

  .kernel-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .update-avail {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11.5px;
    font-weight: 600;
    color: #D97706;
  }

  .update-text {
    font-variant-numeric: tabular-nums;
  }

  .up-to-date {
    font-size: 11px;
    color: #16A34A;
    font-weight: 500;
  }

  :global(.dark) .up-to-date { color: #4ADE80; }

  .kernel-link {
    align-self: flex-start;
    border: none;
    background: transparent;
    color: var(--primary);
    font-size: 11.5px;
    font-weight: 600;
    padding: 0;
    cursor: pointer;
    margin-top: auto;
    flex-shrink: 0;
  }

  .kernel-link:hover {
    text-decoration: underline;
  }
</style>
