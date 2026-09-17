import type { PluginComponent, PluginPermission } from './plugins';
export const permissionKey = (p: PluginPermission) => JSON.stringify([p.capability, p.scope]);
export function canApprove(component: PluginComponent, selected: string[]): boolean {
  if (component.blocked || !component.review || component.configuration?.configured === false) return false;
  return component.permissions.every(p => !p.required || (p.supported && selected.includes(permissionKey(p.request))));
}
export function selectedGrants(component: PluginComponent, selected: string[]): PluginPermission[] {
  return component.permissions.filter(p => p.supported && selected.includes(permissionKey(p.request))).map(p => p.request);
}
export function permissionLabel(capability: string): string {
  return ({
    'plugin.self.read': '读取插件自身信息',
    'network.get': '读取网络内容',
    'network.request': '发送和读取网络请求',
    'records.summary.read': '读取选定记录摘要',
    'plugin.storage.read': '读取插件自己的本地数据',
    'plugin.storage.write': '修改插件自己的本地数据',
    'notifications.post': '显示有来源标识的通知',
    'tasks.schedule': '注册受限后台任务',
    'browser.open': '打开指定网站',
    'browser.callback': '接收一次性浏览器回调',
    'files.selection.read': '读取用户选择的文件',
    'files.selection.write': '写入用户选择的位置',
    'materials.submit': '提交配置材料',
    'secrets.session.receive': '接收临时敏感响应',
    'crypto.session.use': '在当前会话使用敏感材料',
    'runtime.protected.load': '向当前内核运行实例提交受保护配置',
  } as Record<string, string>)[capability] ?? capability;
}
