import type { PluginComponent, PluginPermission } from './plugins';
export const permissionKey = (p: PluginPermission) => JSON.stringify([p.capability, p.scope]);
export function canApprove(component: PluginComponent, selected: string[]): boolean {
  if (component.blocked || !component.review) return false;
  return component.permissions.every(p => !p.required || (p.supported && selected.includes(permissionKey(p.request))));
}
export function selectedGrants(component: PluginComponent, selected: string[]): PluginPermission[] {
  return component.permissions.filter(p => p.supported && selected.includes(permissionKey(p.request))).map(p => p.request);
}
export function permissionLabel(capability: string): string {
  return ({ 'plugin.self.read': '读取插件自身信息', 'network.get': '读取网络内容', 'network.request': '发送和读取网络请求', 'records.summary.read': '读取选定记录摘要' } as Record<string, string>)[capability] ?? capability;
}
