<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { scale, fly, fade } from 'svelte/transition';
  import { elasticOut, cubicOut } from 'svelte/easing';
  import { store } from '$lib/services/store.svelte';
  import AppLogo from '$lib/components/AppLogo.svelte';

  let step = $state(0);
  let selectedMode = $state<'lite' | 'pro'>(store.uiMode);
  let entering = $state(false);
  let enterError = $state<string | null>(null);
  const totalSteps = 4;

  function next() { if (step < totalSteps - 1) step++; }
  function prev() { if (step > 0) step--; }

  async function enterApp(mode: 'lite' | 'pro') {
    if (entering) return;
    entering = true;
    enterError = null;
    try {
      await store.startApp(mode);
    } catch (error) {
      enterError = error instanceof Error ? error.message : String(error);
      entering = false;
    }
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
        <button data-slot="surface-button"
          class="step-dot {i === step ? 'active' : ''} {i < step ? 'done' : ''}"
          onclick={() => step = i}
          aria-label="步骤 {i + 1}"
        ></button>
        {#if i < totalSteps - 1}
          <span class="step-line" class:done={i < step}></span>
        {/if}
      {/each}
    </div>

    <!-- Step 1: Mode selection -->
    {#if step === 0}
      <div transition:fly={{ y: 10, duration: 220, easing: cubicOut }} class="step-content">
        <div class="step-number">01</div>
        <h3 class="step-title">选择界面模式</h3>
        <p class="step-desc">
          简约模式保留日常使用入口；专业模式开放节点、静态配置、规则、连接和调试等完整控制面。
        </p>
        <div class="mode-cards">
          <button data-slot="surface-button"
            onclick={() => selectedMode = 'lite'}
            class="mode-card {selectedMode === 'lite' ? 'selected' : ''}"
            aria-pressed={selectedMode === 'lite'}
          >
            <span class="mode-card-title">简约模式</span>
            <span class="mode-card-desc">概览、订阅、日志和设置</span>
            <span class="mode-card-badge">推荐入门</span>
          </button>
          <button data-slot="surface-button"
            onclick={() => selectedMode = 'pro'}
            class="mode-card {selectedMode === 'pro' ? 'selected' : ''}"
            aria-pressed={selectedMode === 'pro'}
          >
            <span class="mode-card-title">专业模式</span>
            <span class="mode-card-desc">完整运行、配置与诊断入口</span>
            <span class="mode-card-badge pro">高级控制</span>
          </button>
        </div>
        <div class="step-actions">
          <Button variant="default" size="sm" onclick={next} >
            下一步
          </Button>
        </div>
      </div>

    <!-- Step 2: Kernel setup -->
    {:else if step === 1}
      <div transition:fly={{ y: 10, duration: 220, easing: cubicOut }} class="step-content">
        <div class="step-number">02</div>
        <h3 class="step-title">安装内核</h3>
        <p class="step-desc">
          GUI 负责管理，内核负责实际代理。进入应用后，可从概览页的「内核版本」卡片打开版本管理并安装；也可以在「设置 → 内核」选择已有可执行文件。
        </p>
        <p class="step-desc subtle">
          稳定版适合日常使用；beta 和 nightly 版本可在版本管理中按需选择。
        </p>
        <div class="step-actions">
          <Button variant="outline" size="sm" onclick={prev} >上一步</Button>
          <Button variant="default" size="sm" onclick={next} >下一步</Button>
        </div>
      </div>

    <!-- Step 3: Add a proxy source -->
    {:else if step === 2}
      <div transition:fly={{ y: 10, duration: 220, easing: cubicOut }} class="step-content">
        <div class="step-number">03</div>
        <h3 class="step-title">添加代理来源</h3>
        <p class="step-desc">
          推荐在「订阅」页点击「新增」，保存订阅链接后执行同步。同步结果会写入关联的代理配置，并自动更新后续节点数据。
        </p>
        <ul class="tips-list">
          <li>日常使用：新增订阅并同步，后续可启用自动同步</li>
          <li>专业模式：也可在「配置」页导入本地 JSON 或粘贴静态配置</li>
          <li>规则和公共规则可在专业模式的「规则」页继续管理</li>
        </ul>
        <div class="step-actions">
          <Button variant="outline" size="sm" onclick={prev} >上一步</Button>
          <Button variant="default" size="sm" onclick={next} >下一步</Button>
        </div>
      </div>

    <!-- Step 4: Getting started -->
    {:else}
      <div transition:fly={{ y: 10, duration: 220, easing: cubicOut }} class="step-content">
        <div class="step-number">04</div>
        <h3 class="step-title">开启服务</h3>
        <p class="step-desc">
          准备好内核和代理配置后，在概览页点击「开启服务」。应用会启动内核并开启系统代理；若内核已经运行，按钮会显示为「开启系统代理」。
        </p>
        <ul class="tips-list">
          <li>概览页会显示内核、系统代理、TUN 和当前路由模式</li>
          <li>本地公网 IP 会在启动、重启和代理切换后自动重新检测</li>
          <li>简约/专业模式可随时在标题栏或设置中切换</li>
        </ul>
        {#if enterError}
          <div class="enter-error" role="alert">保存界面模式失败：{enterError}</div>
        {/if}
        <div class="step-actions">
          <Button variant="outline" size="sm" onclick={prev}  disabled={entering}>上一步</Button>
          <Button variant="default" size="sm"
            onclick={() => enterApp(selectedMode)}

            disabled={entering}
          >
            {entering ? '正在进入…' : '开始使用'}
          </Button>
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

  .step-desc.subtle {
    font-size: 12px;
    opacity: 0.72;
  }

  .enter-error {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid color-mix(in srgb, var(--destructive) 35%, var(--border));
    border-radius: 8px;
    background: color-mix(in srgb, var(--destructive) 8%, transparent);
    color: var(--destructive);
    font-size: 11.5px;
    line-height: 1.45;
    text-align: left;
  }

  .step-actions {
    display: flex;
    gap: 10px;
    margin-top: 4px;
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
