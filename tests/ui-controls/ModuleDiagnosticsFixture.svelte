<script lang="ts">
  import ModuleDiagnosticsPanel from '$lib/components/tabs/ModuleDiagnosticsPanel.svelte';
  import type { ModuleReport } from '$lib/features/diagnostics/model';
  let calls = 0;
  async function load(): Promise<ModuleReport> {
    calls++;
    if (calls > 1) throw new Error('测试读取失败');
    return {collectedAt: 1700000000000, modules: [
      {id: 'runtime-host', title: '内核托管', state: 'ready', summary: '进程状态：running', error: '上次启动失败，已恢复', facts: [{label:'期望运行',value:'是'}, {label:'当前操作',value:'无'}]},
      {id: 'configuration', title: '配置组合', state: 'unavailable', summary: '读取失败，当前状态未知', error:'配置状态暂不可用', facts:[]},
      {id: 'probe', title: '网络探测', state: 'busy', summary: '探测进行中', facts:[]},
    ]};
  }
</script>
<ModuleDiagnosticsPanel {load} />
