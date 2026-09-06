import type { ProxyMode } from '$lib/types/gui-api';
export const preview = $state({ feedback: '', dark: false, view: 'production', scenario: 'normal', pending: '', noticeTarget: '', notice: '', operationError: false });
export const store = $state({
  uiMode: 'pro', isSwitchingUiMode: false, activeTab: 'overview', settingsSection: 'general', isInitialized: true,
  openSettings(section: string) { this.settingsSection = section; this.activeTab = 'settings'; },
  async switchUIMode(mode: string) { this.uiMode = mode; },
  isNavVisible(key: string) { return this.uiMode === 'pro' || ['overview', 'nodes', 'subscriptions', 'connections', 'logs', 'settings'].includes(key); },
  isNavOperable() { return true; }, isFeatureVisible() { return true; },
});
const now = Date.now();
export const guiState = $state({
  connection: { state: 'connected' as const, processState: 'running', coreAvailable: true, processPid: 12345, startedAtUnixMs: now - 3780000, systemProxyEnabled: true, localProxyHost: '127.0.0.1', localProxyPort: 7890 },
  connectionUpdatedAt: now, connectionError: null,
  coreOverview: { coreState: 'running' as const, version: '0.0.16-rc.202609060636' },
  isInitializing: false, isConnecting: false, isDisconnecting: false, isStartingCore: false, isStoppingCore: false,
  isSelectingPolicy: false,
  get isCoreBusy() { return this.isStartingCore || this.isStoppingCore; },
  isSwitchingSystemProxy: false, isSwitchingTun: false, isSwitchingMode: false,
  get isProcessRunning() { return this.connection.processState === 'running'; },
  get isSystemProxyEnabled() { return this.connection.systemProxyEnabled; },
  get isTunEnabled() { return this.tunStatus.enabled; },
  get isTunDesiredEnabled() { return this.tunStatus.desiredEnabled; },
  get isTunSwitchOn() { return this.isTunEnabled || this.isTunDesiredEnabled; },
  get isCaptureEnabled() { return this.isSystemProxyEnabled || this.isTunEnabled; },
  get isConnected() { return this.isSystemProxyEnabled && this.isTunEnabled; },
  get canStartCore() { return !this.isProcessRunning && !this.isStartingCore; },
  get canRestartCore() { return this.isProcessRunning && !this.isStoppingCore && !preview.pending; },
  get canEnableSystemProxy() { return !this.isSwitchingSystemProxy && !preview.pending; },
  get canDisableSystemProxy() { return !this.isSwitchingSystemProxy && !preview.pending; },
  get canEnableTun() { return !this.isSwitchingTun && !preview.pending; }, get canDisableTun() { return !this.isSwitchingTun && !preview.pending; },
  supportsTrafficStats: true, blockingIssues: [], configNodes: [],
  policyGroups: [{ name: 'Proxy', kind: 'selector', selected: '示例节点', outbounds: [{ tag: '示例节点', type: 'vless', alive: true, delayMs: 38, lastCheckedUnixMs: now }, { tag: '备用节点', type: 'trojan', alive: true, delayMs: 52, lastCheckedUnixMs: now }] }],
  proxyMode: { currentMode: 'rule' as ProxyMode, availableModes: ['global', 'rule', 'direct'] as ProxyMode[] },
  tunStatusError: null,
  tunStatus: { key: 'tun', supported: true, enabled: true, desiredEnabled: true, healthy: true, state: 'running', configSource: 'app' as const, configSourceName: '示例配置', name: 'utun5', mtu: 1500, addr: '10.66.0.1/24', addresses: ['10.66.0.1/24'], autoRoute: true, dualStack: true, strictRoute: false, dnsHijack: true, fakeIpEnabled: false, dnsHijackedQueries: 108, ipv4Egress: { availability: 'available' as 'available' | 'unavailable' | 'unknown', interface: 'en0' }, ipv6Egress: { availability: 'available' as 'available' | 'unavailable' | 'unknown', interface: 'en0' }, networkGeneration: 4, ipv6ToIpv4Fallbacks: 0, managedByConfig: false, lastError: undefined as string | undefined },
  networkProbeLoading: false, networkProbeError: null,
  networkProbe: { ip: '192.0.2.18', region: '示例网络', city: '模拟检测结果', isp: '演示数据' },
  selfTestUpdatedAt: now,
  selfTest: { suggestedFlow: 'ready' as const, ready: true, activeProxyConfigId: 'demo', activeProxyConfigName: '日常网络', warningCount: 0, blockingIssues: [], checks: [{ key: 'activeProxyConfig', status: 'pass' as const, message: '示例配置已加载' }, { key: 'control', status: 'pass' as const, message: '控制接口已响应（模拟）' }] },
  applyScenario(scenario: string) {
    if (preview.pending) return;
    preview.scenario = scenario;
    this.connection.systemProxyEnabled = true;
    this.proxyMode.currentMode = 'rule';
    this.selfTest.activeProxyConfigId = 'demo'; this.selfTest.activeProxyConfigName = '日常网络';
    this.tunStatus.enabled = true; this.tunStatus.desiredEnabled = true;
    this.tunStatus.healthy = scenario !== 'tun-failed';
    this.tunStatus.lastError = scenario === 'tun-failed' ? 'IPv4 默认路由不可用，TUN 出口恢复失败。' : undefined;
    this.tunStatus.ipv4Egress.availability = scenario === 'tun-failed' ? 'unavailable' : 'available';
    this.policyGroups[0].selected = '示例节点';
    this.policyGroups[0].outbounds[0].alive = scenario !== 'node-failed';
    for (const outbound of this.policyGroups[0].outbounds) outbound.lastCheckedUnixMs = Date.now();
    proxyConfigSignal.markChanged();
    preview.feedback = ''; preview.notice = ''; preview.noticeTarget = ''; preview.operationError = false;
  },
  async selectPolicy(groupName: string, tag: string) { return this.choosePolicy(groupName, tag); },
  async refreshNodeStateAfterConfigChange() {}, async refreshSelfTest() {},
  async choosePolicy(groupName: string, tag: string) {
    const group = this.policyGroups.find(group => group.name === groupName);
    if (!group || group.selected === tag || !group.outbounds.some(outbound => outbound.tag === tag)) return;
    return performOperation(`policy:${groupName}`, `已切换到 ${tag}`, () => { group.selected = tag; });
  },
  async chooseProfile(id: string) {
    if (id === this.selfTest.activeProxyConfigId || !['demo', 'work'].includes(id)) return;
    return performOperation('profile', '配置已生效', () => {
      this.selfTest.activeProxyConfigId = id; this.selfTest.activeProxyConfigName = id === 'work' ? '工作配置' : '日常网络';
    });
  },
  async setProxyMode(mode: string) {
    if (preview.pending || mode === this.proxyMode.currentMode || !['global', 'rule', 'direct'].includes(mode)) return;
    this.isSwitchingMode = true;
    try { return await performOperation('mode', '代理模式已生效', () => { this.proxyMode.currentMode = mode as ProxyMode; }); }
    finally { this.isSwitchingMode = false; }
  },
  async toggleSystemProxy() {
    if (preview.pending || this.isSwitchingSystemProxy) return;
    this.isSwitchingSystemProxy = true;
    try { return await performOperation('system-proxy', '系统代理设置已生效', () => { this.connection.systemProxyEnabled = !this.connection.systemProxyEnabled; }); }
    finally { this.isSwitchingSystemProxy = false; }
  },
  async toggleTun() {
    if (preview.pending || this.isSwitchingTun) return;
    this.isSwitchingTun = true;
    try { return await performOperation('tun', 'TUN 设置已生效', () => {
      this.tunStatus.enabled = !this.tunStatus.enabled; this.tunStatus.desiredEnabled = this.tunStatus.enabled;
      this.tunStatus.lastError = this.tunStatus.enabled && !this.tunStatus.healthy ? 'IPv4 默认路由不可用，TUN 出口恢复失败。' : undefined;
    }); } finally { this.isSwitchingTun = false; }
  },
  async restartCore() { this.isStoppingCore = true; await delay(); this.isStoppingCore = false; preview.feedback = '内核已重启（模拟）'; },
  async startCore() { this.connection.processState = 'running'; this.connection.coreAvailable = true; },
  async connect() { this.connection.systemProxyEnabled = true; this.tunStatus.enabled = true; },
  async disconnect() { this.connection.systemProxyEnabled = false; this.tunStatus.enabled = false; },
  async probeNetwork() { this.networkProbeLoading = true; await delay(); this.networkProbeLoading = false; preview.feedback = '网络检测完成（模拟）'; },
  async refreshTunStatus() {}, async refreshAll() { this.connectionUpdatedAt = Date.now(); },
});
let noticeTimer: ReturnType<typeof setTimeout> | undefined;
// The fixture waits for a simulated acknowledgement before changing confirmed state.
async function performOperation(target: string, success: string, apply: () => void) {
  if (preview.pending) return;
  clearTimeout(noticeTimer);
  preview.pending = target; preview.noticeTarget = target; preview.notice = ''; preview.operationError = false;
  try {
    await new Promise(resolve => setTimeout(resolve, 1000));
    if (preview.scenario === 'operation-failed') {
      preview.notice = '切换失败：内核未确认，已保留原设置。'; preview.operationError = true;
    } else { apply(); preview.notice = success; }
    preview.feedback = `${preview.notice}（模拟）`;
    if (!preview.operationError) noticeTimer = setTimeout(() => { if (preview.noticeTarget === target && !preview.pending) preview.notice = ''; }, 3500);
    return preview.operationError ? { ok: false, message: preview.notice } : { ok: true };
  } finally { preview.pending = ''; }
}
function delay() { return new Promise(resolve => setTimeout(resolve, 350)); }
export const coreEvents = $state({ statusTick: 0, stackState: 'started', stackMode: 'Zero Stack' });
export const updater = $state({ prominentUpdateAvailable: false, latestVersion: null });
export const trafficBallPreference = $state({ enabled: false });

const profileListeners = new Set<() => void>();
export const proxyConfigSignal = { onActiveChanged(listener: () => void) { profileListeners.add(listener); return () => { profileListeners.delete(listener); }; }, markChanged() { for (const listener of profileListeners) listener(); } };
