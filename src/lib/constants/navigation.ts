export interface NavTab {
  id: string;
  label: string;
  roles: ('lite' | 'pro')[];
  canHide: boolean;
}

export const NAV_TABS: NavTab[] = [
  { id: 'overview', label: '概览', roles: ['lite', 'pro'], canHide: false },
  { id: 'profiles', label: '配置', roles: ['lite', 'pro'], canHide: true },
  { id: 'subscriptions', label: '订阅', roles: ['lite', 'pro'], canHide: true },
  { id: 'rules', label: '规则', roles: ['lite', 'pro'], canHide: true },
  { id: 'connections', label: '连接', roles: ['lite', 'pro'], canHide: true },
  { id: 'logs', label: '日志', roles: ['lite', 'pro'], canHide: true },
  { id: 'capabilities', label: '能力', roles: ['lite', 'pro'], canHide: true },
  { id: 'settings', label: '设置', roles: ['lite', 'pro'], canHide: false }
];

export const TAB_LABELS: Record<string, string> = {
  overview: '概览',
  profiles: '配置',
  subscriptions: '订阅',
  rules: '规则',
  connections: '连接',
  logs: '日志',
  capabilities: '能力',
  settings: '设置'
};
