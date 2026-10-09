import type { AppConfig, AppTunConfigPatch } from '../../src/lib/types/app-config';

const mode = new URLSearchParams(window.location.search).get('mode');
export const guiState = $state({
  proxyMode: {currentMode: new URLSearchParams(location.search).get('route') === 'global' ? 'global' : 'rule', globalOutbound: 'node-a'},
  isSwitchingMode: false,
  isSelectingPolicy: false,
  setProxyMode: async (next: string, options?: {globalOutbound?: string}) => {
    guiState.isSwitchingMode = true;
    try {
      await new Promise(resolve => setTimeout(resolve, 200));
      if (new URLSearchParams(location.search).has('global-failure')) return {ok:false,message:'全局出口应用失败，保留原出口'};
      guiState.proxyMode = {currentMode:next,globalOutbound:options?.globalOutbound ?? guiState.proxyMode.globalOutbound};
      window.dispatchEvent(new CustomEvent('fixture-save',{detail:{mode:next,globalOutbound:options?.globalOutbound}}));
      return {ok:true};
    } finally { guiState.isSwitchingMode = false; }
  },
  blockingIssues: [] as string[],
  isTunEnabled: mode !== 'stopped',
  isSwitchingTun: false,
  tunStatus: { configSource: mode === 'profile' ? 'profile' : 'app', configSourceName: '测试配置' },
  refreshTunStatus: async () => {},
});
window.addEventListener('fixture-mode-changed', event => {
  guiState.proxyMode = {...guiState.proxyMode,currentMode:(event as CustomEvent<string>).detail};
});
export const store = $state({ interactionSurface: { actions: new Map(), navigation: new Map() }, isActionOperable: (_action: string) => true, selectedTheme: 'light', isSwitchingUiMode: false, switchUIMode: async (_mode: string) => {}, refreshInteractionSurface: async () => {}, isNavOperable: (_key: string) => true, isNavVisible: (key: string) => store.interactionSurface.navigation.get(key)?.visible ?? (store.uiMode === 'pro' || ['overview', 'nodes', 'subscriptions', 'logs', 'plugins', 'settings'].includes(key)), uiMode: 'pro' as 'lite' | 'pro', activeTab: 'settings', settingsSection: 'tun', isInitialized: true, openSettings: (_section: string) => {} });
export type TunDnsHijackReadiness = {
  state: string; code?: string; message: string;
  features: { tunDualStack: { state: string }; tunDnsHijack: { state: string } };
};
export const inspectTunDnsHijackReadiness = async (): Promise<TunDnsHijackReadiness> => ({
  state: 'ready', message: 'ready',
  features: { tunDualStack: { state: 'supported' }, tunDnsHijack: { state: 'supported' } },
});
let config = {
  tun: { enabled: mode !== 'stopped', name: null, addr: '10.66.0.1/24', mask: '255.255.255.0',
    secondaryAddr: null, tag: 'tun', mtu: 1500, includeCidrs: [], excludeCidrs: ['16.0.0.0/8'],
    dualStack: true, dnsHijack: false },
  dns: { enabled: false, config: null, dnsHijack: false },
} as unknown as AppConfig;

export const getTunConfig = async () => structuredClone(config);
export async function applyFixtureTun(patch: AppTunConfigPatch): Promise<AppConfig> {
  // Keep the UI busy long enough to test duplicate-submit protection.
  await new Promise((resolve) => setTimeout(resolve, 400));
  if (mode === 'failure') throw { code: 'internal', message: '路由安装失败；已恢复旧 TUN 配置' };
  config = { ...config, tun: { ...config.tun, ...patch,
    name: patch.name === null ? undefined : patch.name ?? config.tun.name,
    secondaryAddr: patch.secondaryAddr === null ? undefined : patch.secondaryAddr ?? config.tun.secondaryAddr,
  } };
  window.dispatchEvent(new CustomEvent('fixture-save', { detail: config.tun }));
  return structuredClone(config);
}

export const getGuiTunStatus = async () => ({ ...guiState.tunStatus, ipv4Egress:{availability:'available'},ipv6Egress:{availability:'unavailable'} });
