<script lang="ts">
  import { scale, fly, fade } from 'svelte/transition';
  import { elasticOut, cubicOut } from 'svelte/easing';
  import { store } from '$lib/services/store.svelte';
  import { Button } from '$lib/components/ui/button';
</script>

<section 
  transition:fade={{ duration: 500 }}
  class="flex-1 w-full flex flex-col items-center justify-center p-8"
>
  <div
    transition:scale={{ delay: 100, duration: 600, easing: elasticOut, start: 0.8 }}
    class="text-center mb-10"
  >
    <div class="w-16 h-16 mx-auto mb-4 rounded-2xl bg-gradient-to-br from-primary/20 to-primary/5 flex items-center justify-center border border-primary/20">
      <span class="text-2xl">🚀</span>
    </div>
    <h2 class="text-lg font-black text-foreground mb-2 tracking-wide uppercase">
      欢迎使用 ZNet Sink
    </h2>
    <p class="text-sm text-muted-foreground font-medium">
      请选择最契合您习惯的交互工作流开始
    </p>
  </div>

  <div class="grid grid-cols-2 gap-5 w-[520px]">
    <div
      transition:fly={{ delay: 200, x: -30, duration: 500, easing: cubicOut }}
      class="group"
    >
      <Button
        variant="secondary"
        onclick={() => store.startApp('lite')}
        class="w-full h-auto p-5 flex flex-col items-start gap-2 text-left hover:border-primary/30 hover:shadow-lg hover:shadow-primary/10 transition-all duration-300 hover:-translate-y-0.5"
      >
        <span class="text-2xl mb-1">🍃</span>
        <span class="font-bold text-sm text-foreground">简约模式 (Lite)</span>
        <span class="text-[11px] text-muted-foreground leading-normal font-medium">
          仅提供启动开关、订阅管理以及节点切换。界面极简，开箱即用。
        </span>
      </Button>
    </div>

    <div
      transition:fly={{ delay: 300, x: 30, duration: 500, easing: cubicOut }}
      class="group"
    >
      <Button
        variant="secondary"
        onclick={() => store.startApp('pro')}
        class="w-full h-auto p-5 flex flex-col items-start gap-2 text-left hover:border-primary/30 hover:shadow-lg hover:shadow-primary/10 transition-all duration-300 hover:-translate-y-0.5"
      >
        <span class="text-2xl mb-1">🎛️</span>
        <span class="font-bold text-sm text-foreground">专业模式 (Pro)</span>
        <span class="text-[11px] text-muted-foreground leading-normal font-medium">
          全能力开放。解锁实时流量走势、精细并发连接树、插件扩展以及分流规则重写。
        </span>
      </Button>
    </div>
  </div>

  <div
    transition:fly={{ delay: 500, y: 20, duration: 400, easing: cubicOut }}
    class="mt-8 px-5 py-3.5 rounded-xl bg-emerald-500/10 border border-emerald-500/20 max-w-[480px]"
  >
    <span class="text-[11px] text-emerald-400 font-medium leading-relaxed">
      💡 提示：首次启动后，可在「设置」中随时切换模式。如果内核未启动，默认以简约模式展示基础功能。
    </span>
  </div>
</section>
