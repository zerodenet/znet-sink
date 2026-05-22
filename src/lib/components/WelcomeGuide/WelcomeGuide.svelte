<script lang="ts">
  import { scale, fly, fade } from 'svelte/transition';
  import { elasticOut, cubicOut } from 'svelte/easing';
  import { store } from '$lib/services/store.svelte';

  function openCoreSettings() {
    store.openSettings('core');
  }
</script>

<section
  transition:fade={{ duration: 300 }}
  class="flex-1 w-full flex flex-col items-center justify-center"
>
  <div
    transition:scale={{ delay: 80, duration: 500, easing: elasticOut, start: 0.85 }}
    class="text-center mb-8"
  >
    <div class="welcome-icon">
      <img src="/favicon.png" alt="ZNet Sink" style="width: 36px; height: 36px;" class="rounded-xl opacity-90" />
    </div>
    <h2 class="welcome-title">ZNet Sink</h2>
    <p class="welcome-sub">先配置内核，再进入界面</p>
  </div>

  <div class="welcome-panel">
    <div class="welcome-note">
      GUI 和内核是分离的。你可以指定本地路径，或者填入远端下载后的内核位置。
    </div>

    <button
      transition:fly={{ delay: 140, y: 8, duration: 280, easing: cubicOut }}
      onclick={openCoreSettings}
      class="primary-action"
    >
      去配置内核
    </button>

    <div class="mode-row">
      <button onclick={() => store.startApp('lite')} class="mode-link">进入 Lite</button>
      <button onclick={() => store.startApp('pro')} class="mode-link">进入 Pro</button>
    </div>
  </div>
</section>

<style>
  .welcome-icon {
    width: 56px;
    height: 56px;
    border-radius: 14px;
    background: var(--card);
    border: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: center;
    margin: 0 auto 12px;
  }

  .welcome-title {
    font-size: 16px;
    font-weight: 700;
    color: var(--foreground);
    margin-bottom: 4px;
  }

  .welcome-sub {
    font-size: 12.5px;
    color: var(--muted-foreground);
  }

  .welcome-panel {
    width: min(460px, 100%);
    display: grid;
    gap: 12px;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--card);
  }

  .welcome-note {
    font-size: 12px;
    color: var(--muted-foreground);
    line-height: 1.6;
  }

  .primary-action {
    height: 36px;
    border: none;
    border-radius: 8px;
    background: var(--primary);
    color: var(--primary-foreground);
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
  }

  .mode-row {
    display: flex;
    justify-content: center;
    gap: 14px;
  }

  .mode-link {
    border: none;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 12px;
    cursor: pointer;
  }
</style>
