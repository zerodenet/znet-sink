import { moduleCatalog, productName } from 'virtual:znet-product-metadata';
import { readRegisteredModules } from '$lib/features/diagnostics/registry';
import { getCoreProcessStatus } from './core';
import { getConfigCompositionReport } from './config';
import { guiState } from './gui-state.svelte';
import { collectModules, type ModuleStatus } from '$lib/features/diagnostics/model';
export function getModuleDiagnostics() {
  return collectModules([
    {id: 'runtime-host', title: '内核托管', async read(): Promise<ModuleStatus> {
      const process = await getCoreProcessStatus();
      const host = process.host;
      return {id: 'runtime-host', title: '内核托管', state: host?.operation ? 'busy' : process.state === 'running' ? 'ready' : process.state === 'failed' ? 'error' : 'idle',
        summary: `进程状态：${process.state}`, error: host?.lastError ?? process.lastError,
        facts: [{label: '期望运行', value: host ? host.desiredRunning ? '是' : '否' : '未知'}, {label: '当前操作', value: host?.operation ?? '无'}, {label: '托管代次', value: host ? String(host.generation) : '未知'}]};
    }},
    {id: 'configuration', title: '配置组合', async read(): Promise<ModuleStatus> {
      const report = await getConfigCompositionReport();
      return {id: 'configuration', title: '配置组合', state: report ? 'ready' : 'idle', summary: report ? '最近一次成功组合；不代表已在内核生效' : '本次运行尚无组合记录',
        facts: report?.layers.map((layer) => ({label: layer.source, value: layer.changedPaths.length ? layer.changedPaths.join('、') : '未改变'})) ?? []};
    }},
  ], [{id: 'product', title: '产品组合', state: 'ready', summary: productName, facts: moduleCatalog.map(tool => ({label: tool.title, value: '已编译'}))}, ...guiState.moduleDiagnostics(), ...readRegisteredModules(moduleCatalog)]);
}
