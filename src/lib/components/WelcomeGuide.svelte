<script lang="ts">
  import { store } from '$lib/services/store.svelte';
</script>

<section class="flex-1 w-full bg-card border border-card-border rounded-2xl flex flex-col items-center justify-center p-6 shadow-sm animate-fade-in">
  <div class="text-center mb-8">
    <h2 class="text-base font-black text-foreground mb-1 tracking-wide uppercase">欢迎使用 ZNet Sink</h2>
    <p class="text-muted-foreground text-xs font-medium">请选择最契合您习惯的交互工作流开始</p>
  </div>

  <div class="grid grid-cols-2 gap-4 w-[500px]">
    <button onclick={() => store.startApp('lite')} class="p-5 border border-card-border bg-muted/30 rounded-2xl text-left flex flex-col gap-2 transition-all hover:border-foreground/20 hover:bg-muted/60 hover:translate-y-[-1px] shadow-sm">
      <span class="text-xl">🍃</span>
      <span class="font-bold text-sm text-foreground">简约模式 (Lite)</span>
      <span class="text-xs text-muted-foreground leading-normal font-medium">仅提供启动开关、订阅管理以及节点切换。界面极简，开箱即用。</span>
    </button>

    <button onclick={() => store.startApp('pro')} class="p-5 border border-card-border bg-muted/30 rounded-2xl text-left flex flex-col gap-2 transition-all hover:border-foreground/20 hover:bg-muted/60 hover:translate-y-[-1px] shadow-sm">
      <span class="text-xl">🎛️</span>
      <span class="font-bold text-sm text-foreground">专业模式 (Pro)</span>
      <span class="text-xs text-muted-foreground leading-normal font-medium">全能力开放。解锁实时流量走势、精细并发连接树、插件扩展以及分流规则重写。</span>
    </button>
  </div>
</section>
