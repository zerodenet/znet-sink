<script lang="ts">
  import type { ProxyNode } from '../types/protocol';

  interface NodeGridProps {
    nodes: ProxyNode[];
    selectedNodeId: string;
    onSelectNode: (id: string) => void;
  }

  let { nodes, selectedNodeId, onSelectNode }: NodeGridProps = $props();
</script>

<!-- 主磁贴展示区：通过自适应间距，完美承载大磁贴平铺 -->
<div class="flex-1 w-full p-1 grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-3 overflow-y-auto content-start">
  {#each nodes as node}
    <!-- 高级磁贴大按钮 -->
    <button 
      onclick={() => onSelectNode(node.id)}
      class="p-4 rounded-2xl border text-left transition-all duration-300 flex flex-col justify-between h-28 relative group overflow-hidden
             {selectedNodeId === node.id 
               ? 'bg-zinc-800/80 border-emerald-500/50 shadow-xl shadow-emerald-500/[0.02] translate-y-[-2px]' 
               : 'bg-[#121418]/90 border-white/[0.03] hover:border-white/[0.08] hover:bg-[#16181d] shadow-md hover:translate-y-[-1px]'}"
    >
      <!-- 激活状态下的背景微弱极光浸透（GFC 的高级细节） -->
      {#if selectedNodeId === node.id}
        <div class="absolute -right-6 -bottom-6 w-20 h-20 bg-emerald-500/10 blur-2xl rounded-full"></div>
      {/if}

      <!-- 磁贴上半部分信息 -->
      <div class="w-full flex justify-between items-start z-10">
        <div class="flex flex-col gap-0.5 max-w-[70%]">
          <span class="text-xs font-bold truncate tracking-wide {selectedNodeId === node.id ? 'text-emerald-400' : 'text-zinc-100'}">
            {node.name}
          </span>
          <span class="text-[10px] text-zinc-500 font-mono truncate">{node.domain}</span>
        </div>
        
        <!-- 协议微章 -->
        <span class="text-[9px] font-black font-mono px-2 py-0.5 rounded-lg bg-zinc-950/60 border border-white/[0.04] text-zinc-400">
          {node.protocol}
        </span>
      </div>

      <!-- 磁贴下半部分：超清晰延迟数据 -->
      <div class="w-full flex justify-between items-center z-10 pt-2 border-t border-white/[0.02]">
        <span class="text-[9px] text-zinc-600 font-mono uppercase tracking-widest">Latency</span>
        <span class="text-xs font-mono font-black tracking-tight {selectedNodeId === node.id ? 'text-emerald-400' : 'text-emerald-500'}">
          {node.delay} <span class="text-[9px] font-sans font-normal opacity-70">ms</span>
        </span>
      </div>
    </button>
  {/each}
</div>
