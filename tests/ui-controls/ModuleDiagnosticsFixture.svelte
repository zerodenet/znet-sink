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
      {id: 'composition', title: '配置覆盖记录', state: 'ready', summary: '最近一次成功组合；不代表已在内核生效', facts:[{label:'公共测速地址',value:'/runtime/latency_test_url、/outbound_groups/automatic/latency_test_url'}, {label:'保存的节点选择',value:'未改变'}, {label:'本地绕过',value:'/route/bypass'}]},
      {id: 'product', title: '产品组合', state: 'ready', summary: 'desktop', facts:[{label:'DNS 与 Fake-IP',value:'已编译'},{label:'路由追踪',value:'已编译'},{label:'节点测速任务',value:'已编译'}]},
      {id: 'runtime', title: '连接与接管观测', state: 'ready', summary: '内核可用', updatedAt:1700000000000, facts:[{label:'系统代理',value:'开启'},{label:'TUN 保存意图',value:'关闭'}]},
      {id:'nodes',title:'策略与配置节点',state:'ready',summary:'3 个运行策略组 · 17 个配置节点',facts:[]},
      {id:'dns',title:'DNS 与 Fake-IP',state:'idle',summary:'页面已关闭；保留最后一次观测摘要',facts:[]},
      {id:'route',title:'路由追踪',state:'idle',summary:'页面已关闭；保留最后一次观测摘要',facts:[]},
    ]};
  }
</script>
<ModuleDiagnosticsPanel {load} />
