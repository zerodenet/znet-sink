<script lang="ts">
  import { scale, fly, fade } from 'svelte/transition';
  import { elasticOut, cubicOut } from 'svelte/easing';
  import { store } from '$lib/services/store.svelte';
  import AppLogo from '$lib/components/AppLogo.svelte';

  let step = $state(0);
  let selectedMode = $state<'lite' | 'pro'>('lite');
  const totalSteps = 3;

  function next() { if (step < totalSteps - 1) step++; }
  function prev() { if (step > 0) step--; }

  async function enterApp(mode: 'lite' | 'pro') {
    await store.startApp(mode);
  }
</script>

<section
  transition:fade={{ duration: 300 }}
  class="flex-1 w-full flex flex-col items-center justify-center"
>
  <div
    transition:scale={{ delay: 80, duration: 500, easing: elasticOut, start: 0.85 }}
    class="text-center mb-6"
  >
    <div class="welcome-icon">
      <AppLogo size={32} class="welcome-logo" />
    </div>
    <h2 class="welcome-title">ZNet Sink</h2>
    <p class="welcome-sub">零域网络代理客户端</p>
  </div>

  <div class="welcome-panel">
    <!-- Step indicators -->
    <div class="step-dots">
      {#each Array(totalSteps) as _, i}
        <button
          class="step-dot {i === step ? 'active' : ''} {i < step ? 'done' : ''}"
          onclick={() => step = i}
          aria-label="步骤 {i + 1}"
        ></button>
        {#if i < totalSteps - 1}
          <span class="step-line" class:done={i < step}></span>
        {/if}
      {/each}
    </div>

    <!-- Step 1: Kernel setup -->
    {#if step === 0}
      <div transition:fly={{ y: 10, duration: 220, easing: cubicOut }} class="step-content">
        <div class="step-number">01</div>
        <h3 class="step-title">配置内核</h3>
        <p class="step-desc">
          GUI 和内核是分离的。内核负责代理引擎，系统代理由 GUI 作为外置开关设置。
        </p>
        <p class="step-desc" style="font-size: 12px; opacity: 0.7;">
          完成引导后，在「设置 → 内核」中指定内核路径即可开始使用。
        </p>
        <div class="step-actions">
          <button onclick={next} class="primary-action">
            下一步
          </button>
        </div>
      </div>

    <!-- Step 2: Mode selection -->
    {:else if step === 1}
      <div transition:fly={{ y: 10, duration: 220, easing: cubicOut }} class="step-content">
        <div class="step-number">02</div>
        <h3 class="step-title">选择界面模式</h3>
        <p class="step-desc">
          根据你的使用习惯选择模式。随时可以在标题栏切换。
        </p>
        <div class="mode-cards">
          <button onclick={() => selectedMode = 'lite'} class="mode-card {selectedMode === 'lite' ? 'selected' : ''}">
            <span class="mode-card-title">简约模式</span>
            <span class="mode-card-desc">概览、配置、订阅、设置</span>
            <span class="mode-card-badge">推荐入门</span>
          </button>
          <button onclick={() => selectedMode = 'pro'} class="mode-card {selectedMode === 'pro' ? 'selected' : ''}">
            <span class="mode-card-title">专业模式</span>
            <span class="mode-card-desc">全部功能：连接、规则、日志、能力</span>
            <span class="mode-card-badge pro">高级用户</span>
          </button>
        </div>
        <div class="step-actions">
          <button onclick={prev} class="secondary-action">上一步</button>
          <button onclick={next} class="primary-action">下一步</button>
        </div>
      </div>

    <!-- Step 3: Getting started -->
    {:else}
      <div transition:fly={{ y: 10, duration: 220, easing: cubicOut }} class="step-content">
        <div class="step-number">03</div>
        <h3 class="step-title">快速开始</h3>
        <p class="step-desc">
          配置好内核后，在概览页点击「一键开启服务」即可启动内核并设置系统代理。
          以下是一些建议：
        </p>
        <ul class="tips-list">
          <li>先在「设置 → 内核」中配置内核路径</li>
          <li>在「配置」页添加代理配置文件（支持本地 JSON 或粘贴内容）</li>
          <li>在「订阅」页添加订阅 URL，自动获取最新节点</li>
          <li>配置完毕后，在「概览」页点击「一键开启服务」启动内核并设置系统代理</li>
        </ul>
        <div class="step-actions">
          <button onclick={prev} class="secondary-action">上一步</button>
          <button onclick={() => enterApp(selectedMode)} class="primary-action">
            开始使用
          </button>
        </div>
      </div>
    {/if}
  </div>
</section>

<style>
  .welcome-icon {
    --logo-radius: 8px;
    width: 52px;
    height: 52px;
    border-radius: 13px;
    background: var(--card);
    border: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: center;
    margin: 0 auto 10px;
  }

  .welcome-title {
    font-size: 16px;
    font-weight: 700;
    color: var(--foreground);
    margin-bottom: 2px;
  }

  .welcome-sub {
    font-size: 12.5px;
    color: var(--muted-foreground);
  }

  .welcome-panel {
    width: min(480px, 100%);
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 20px 22px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--card);
  }

  /* Step indicators */
  .step-dots {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0;
  }

  .step-dot {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: 2px solid var(--border);
    background: var(--muted);
    cursor: pointer;
    transition: all 0.2s ease;
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
  }

  .step-dot::after {
    content: '';
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--muted-foreground);
    opacity: 0.3;
    transition: all 0.2s ease;
  }

  .step-dot.active {
    border-color: var(--primary);
    background: var(--primary);
  }

  .step-dot.active::after {
    background: var(--primary-foreground);
    opacity: 1;
  }

  .step-dot.done {
    border-color: #22C55E;
    background: rgba(34, 197, 94, 0.12);
  }

  .step-dot.done::after {
    background: #22C55E;
    opacity: 1;
  }

  .step-line {
    width: 36px;
    height: 2px;
    background: var(--border);
    transition: background 0.2s ease;
  }

  .step-line.done {
    background: #22C55E;
  }

  /* Step content */
  .step-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    text-align: center;
  }

  .step-number {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--muted-foreground);
    opacity: 0.5;
  }

  .step-title {
    font-size: 15px;
    font-weight: 700;
    color: var(--foreground);
  }

  .step-desc {
    font-size: 12.5px;
    color: var(--muted-foreground);
    line-height: 1.65;
    max-width: 380px;
  }

  .step-actions {
    display: flex;
    gap: 10px;
    margin-top: 4px;
  }

  .primary-action {
    height: 36px;
    padding: 0 20px;
    border: none;
    border-radius: 8px;
    background: var(--primary);
    color: var(--primary-foreground);
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
    transition: opacity 0.13s ease;
  }

  .primary-action:hover { opacity: 0.88; }

  .secondary-action {
    height: 36px;
    padding: 0 16px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.13s ease, color 0.13s ease;
  }

  .secondary-action:hover {
    background: var(--muted);
    color: var(--foreground);
  }

  /* Mode cards */
  .mode-cards {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    width: 100%;
  }

  .mode-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 5px;
    padding: 14px 10px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--muted);
    cursor: pointer;
    transition: border-color 0.15s ease, background 0.15s ease;
    text-align: center;
  }

  .mode-card:hover {
    border-color: var(--primary);
    background: var(--card);
  }

  .mode-card.selected {
    border-color: var(--primary);
    background: var(--card);
    box-shadow: 0 0 0 1px var(--primary);
  }

  .mode-card-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--foreground);
  }

  .mode-card-desc {
    font-size: 11px;
    color: var(--muted-foreground);
  }

  .mode-card-badge {
    font-size: 10px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 4px;
    background: rgba(34, 197, 94, 0.1);
    color: #16A34A;
  }

  .mode-card-badge.pro {
    background: rgba(168, 85, 247, 0.1);
    color: #A855F7;
  }

  /* Tips list */
  .tips-list {
    list-style: none;
    padding: 0;
    margin: 0;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
  }

  .tips-list li {
    font-size: 12px;
    color: var(--muted-foreground);
    padding-left: 18px;
    position: relative;
    line-height: 1.5;
  }

  .tips-list li::before {
    content: '';
    position: absolute;
    left: 2px;
    top: 7px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--primary);
    opacity: 0.5;
  }
</style>
