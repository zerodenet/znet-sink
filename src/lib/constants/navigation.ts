export interface NavTab {
  id: string;
  label: string;
  comingSoon?: boolean;
}

export const NAV_TABS: NavTab[] = [
  { id: 'overview', label: '概览' },
  { id: 'nodes', label: '节点' },
  { id: 'profiles', label: '代理配置' },
  { id: 'subscriptions', label: '订阅' },
  { id: 'rules', label: '规则' },
  { id: 'connections', label: '连接' },
  { id: 'logs', label: '日志' },
  { id: 'settings', label: '设置' },
  { id: 'debug', label: '调试' },
];

export const TAB_LABELS: Record<string, string> = {
  overview: '概览',
  nodes: '节点',
  profiles: '代理配置',
  subscriptions: '订阅',
  rules: '规则',
  connections: '连接',
  logs: '日志',
  settings: '设置',
  debug: '调试',
};
