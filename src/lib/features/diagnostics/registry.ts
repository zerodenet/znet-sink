import type { ModuleStatus } from './model';
/** Local diagnostic readers only. This registry cannot start or control modules. */
const readers = new Map<string, {token: symbol; read: () => ModuleStatus}>();
const closed = new Map<string, ModuleStatus>();
export function registerModule(id: string, read: () => ModuleStatus): () => void {
  const token = Symbol(id);
  readers.set(id, {token, read});
  closed.delete(id);
  return () => {
    if (readers.get(id)?.token !== token) return;
    const last = read();
    closed.set(id, {...last, state: 'idle', summary: '页面已关闭；保留最后一次观测摘要'});
    readers.delete(id);
    while (closed.size > 16) closed.delete(closed.keys().next().value!);
  };
}
export function readRegisteredModules(catalog: readonly {id: string; title: string}[] = []): ModuleStatus[] {
  const active = [...readers.entries()].map(([id, {read}]): ModuleStatus => {
    try { return read(); }
    catch { return {id, title: id, state: 'unavailable', summary: '本地模块诊断读取失败', facts: []}; }
  });
  const result = active.concat([...closed.values()]);
  for (const {id, title} of catalog) {
    if (!result.some((entry) => entry.id === id)) result.push({id, title, state: 'idle', summary: '按需加载，尚未打开对应页面', facts: []});
  }
  return result;
}
