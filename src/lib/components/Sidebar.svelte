<script lang="ts">
  import type { CoreStatus } from '../types/protocol';

  interface SidebarProps {
    status: CoreStatus;
    onToggleCore: () => void;
  }

  let { status, onToggleCore }: SidebarProps = $props();
</script>

<aside class="w-14 h-full bg-[#121418] border-r border-zinc-800/40 flex flex-col items-center py-4 justify-between flex-shrink-0">
  <!-- 上部：内核开关状态 -->
  <div class="flex flex-col items-center gap-4">
    <!-- 品牌微标 -->
    <div class="w-7 h-7 rounded-lg bg-zinc-800 flex items-center justify-center font-bold text-zinc-200 text-xs border border-zinc-700/50">
      Z
    </div>
    
    <!-- 内核启停按钮 -->
    <button 
      onclick={onToggleCore}
      class="w-8 h-8 rounded-xl flex items-center justify-center border text-base transition-all duration-200
             {status.running ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400' : 'bg-zinc-900 border-zinc-800 text-zinc-500 hover:text-zinc-300'}"
      title={status.running ? "暂停内核" : "启动内核"}
    >
      ⚙️
    </button>
  </div>

  <!-- 下部：TUN / 系统代理模式快捷图标 -->
  <div class="flex flex-col gap-2">
    <button class="w-7 h-7 rounded-lg bg-zinc-800 text-[10px] text-emerald-400 font-mono font-bold border border-zinc-700/40" title="增强 TUN 已开启">
      TUN
    </button>
    <button class="w-7 h-7 rounded-lg text-[10px] text-zinc-600 font-mono hover:text-zinc-400" title="系统代理">
      SYS
    </button>
  </div>
</aside>
