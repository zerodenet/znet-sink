import type { PluginComponent, PluginPermission } from './plugins';
export const permissionKey = (p: PluginPermission) => JSON.stringify([p.capability, p.scope]);
export function initialPermissionSelection(component: PluginComponent): string[] {
  return component.permissions
    .filter(permission => permission.supported && (permission.granted || permission.required))
    .map(permission => permissionKey(permission.request));
}
export function supportedPermissionSelection(component: PluginComponent): string[] {
  return component.permissions
    .filter(permission => permission.supported)
    .map(permission => permissionKey(permission.request));
}
export function requiredPermissionSelection(component: PluginComponent): string[] {
  return component.permissions
    .filter(permission => permission.supported && permission.required)
    .map(permission => permissionKey(permission.request));
}
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
    'network.configured.request': '访问插件已配置的服务来源',
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
    'crypto.device.use': '使用本机设备密钥执行密码学操作',
    'secrets.persistent.read': '读取本机加密凭据',
    'secrets.persistent.write': '保存或删除本机加密凭据',
    'subscriptions.manage': '管理插件提供的托管订阅',
    'runtime.protected.load': '向当前内核运行实例提交受保护配置',
  } as Record<string, string>)[capability] ?? capability;
}
