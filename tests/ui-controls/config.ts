// In-memory fixture only: the UI suite must never call Tauri, DNS or a kernel.
import type { CommonRuleBindingInput, RuleSetSummary, RuleSetUpsert } from '../../src/lib/types/domain';
import type { CoreKernelInfo } from '../../src/lib/types/core';

const names = ['私有网络地址', '中国大陆域名', '中国大陆 IP', 'GFW 域名'];
let items: RuleSetSummary[] = names.map((name, index) => ({
  id: String(index), name, enabled: true, builtIn: true, editableRuleCount: 0,
  sourceState: {}, updatedAtUnixMs: 0,
  commonBinding: { enabled: true, action: index === 3 ? 'proxy' : 'direct', order: (index + 1) * 10 },
  artifact: { path: '', majorVersion: 1, minorVersion: 0, checksum: 1, fileSize: 1024, entryCount: index === 1 ? 111868 : 18, builtAtUnixMs: 0 },
}));
export const listRuleSets = async () => structuredClone(items);
export const getCommonRuleInjectionStatus = async () => ({ enabled: true, effective: true, eligibleCount: 4, injectedCount: 4 });
export const setCommonRuleInjectionEnabled = getCommonRuleInjectionStatus;
export const setCommonRuleBinding = async ({ ruleSetId, ...commonBinding }: CommonRuleBindingInput) => {
  const item = items.find((item) => item.id === ruleSetId)!;
  item.commonBinding = commonBinding;
  return structuredClone(item);
};
export const getRuleSet = async () => { throw new Error('Full rule content must not be loaded by this suite'); };
export const removeRuleSet = async (id: string) => { items = items.filter((item) => item.id !== id); };
export const upsertRuleSet = async (input: RuleSetUpsert) => {
  window.dispatchEvent(new CustomEvent('fixture-save', { detail: input }));
};
export const updateAllRuleSets = async () => ({ total: 4, updated: 4, unchanged: 0, failed: 0 });
export const updateBuiltinRuleSets = updateAllRuleSets;
export const updateRuleSet = async () => items[0];
export const getAppErrorMessage = (error: unknown, fallback: string) => (error as { message?: string })?.message ?? fallback;
export const getAppErrorInfo = (error: unknown, fallback: string) => ({ code: (error as { code?: string })?.code, message: getAppErrorMessage(error, fallback) });
import { getTunConfig } from './tun-state.svelte';
export { applyFixtureTun as applyTunSettings } from './tun-state.svelte';
const endpointConfig = () => ({ localProxy: {
  host: '127.0.0.2', port: 8899,
  sourceProxyConfigId: new URLSearchParams(location.search).has('custom') ? 'custom-profile' : null,
} });
export const getAppConfig = async () => {
  const panel = new URLSearchParams(location.search).get('panel');
  if (panel === 'settings' || panel === 'logs') return { ...(await getTunConfig()), core: { autoStart: true, autoConnect: true, cleanupProxyOnExit: true }, ui: { uiMode: 'pro', hiddenMenuKeys: [] }, localProxy: { host: '127.0.0.1', port: 7890, bypass: ['localhost', '127.*'] }, urlTest: { toleranceMs: 50 } };
  if (panel === 'endpoint') return endpointConfig();
  if (panel === 'kernel') return { core: { kernel: 'zero', executablePath: '/fixture/zero', networkProbeUrls: ['https://example.test'] } };
  return getTunConfig();
};
export const getCoreConfigSnapshot = async (): Promise<CoreKernelInfo> => ({ kernel:'zero', executableExists:true, executablePath:'/fixture/zero', recommendedInstallDir:'/fixture', hasActiveConfig:true, warnings:[] });
export const getCoreProcessStatus = async () => ({ state:'running' });
export const getGuiCoreHealth = async () => ({ engineVersion:'0.0.17-rc.1' });
export const updateAppConfig = async (input?: unknown) => {
  if (new URLSearchParams(location.search).get('panel') === 'endpoint') {
    window.dispatchEvent(new CustomEvent('fixture-save', { detail: input }));
    return input;
  }
  window.dispatchEvent(new Event('fixture-unexpected-config-write'));
  throw new Error('Install must not write app settings a second time');
};
export const guiExportDiagnostics = async () => ({ path: 'fixture' });
export const restartCoreProcess = async () => { throw new Error('The UI fixture cannot restart a core'); };

export const getCorePolicies = async () => ({ groups: [] });
export const getRuntimePerformanceSnapshot = async () => ({ core: null });
export const getGuiStackStatus = async () => ({ key: 'stack', supported: true, enabled: true });

import { proxyConfigSignal } from '../../src/lib/services/proxy-config-signal.svelte';
import type { ProxyConfigProfile, SubscriptionProfile } from '../../src/lib/types/domain';
const profiles: ProxyConfigProfile[] = ['main', 'work'].map((id, i) => ({
  id, name: i ? '工作配置' : '日常网络配置', active: !i, kernel: 'zero', format: 'zero', updatedAtUnixMs: 1,
  content: { route: { final: { type: 'outbound', outbound: 'proxy' } } },
  capabilities: {} as ProxyConfigProfile['capabilities'],
}));
export const listProxyConfigs = async () => { void proxyConfigSignal.revision; return structuredClone(profiles); };
export const listSubscriptions = async (): Promise<SubscriptionProfile[]> => new URLSearchParams(location.search).get('mode') === 'source-failure'
  ? profiles.map(profile => ({id:`sub-${profile.id}`,name:`订阅-${profile.name}`,url:'https://example.test/sub',enabled:true,kernel:'zero',format:'zero',targetProxyConfigId:profile.id,policySelections:{},updatedAtUnixMs:1}))
  : [];
export const setActiveProxyConfig = async (id: string) => {
  if (new URLSearchParams(location.search).get('mode') === 'source-failure' && id === 'work') throw new Error('配置未通过校验');
  for (const profile of profiles) profile.active = profile.id === id;
  proxyConfigSignal.markChanged(true);
  return structuredClone(profiles.find(profile => profile.id === id)!);
};
export const syncSubscription = async (): Promise<SubscriptionProfile> => { throw new Error('No subscription network calls in this fixture'); };

export const guiLogPaths = async () => ({ logFile: '/fixture/logs/gui.log.jsonl', coreLogFile: '/fixture/logs/core.log.jsonl', logsDir: '/fixture/logs', dataDir: '/fixture' });
export const appendLog = async (_input: unknown) => {};
import type { LogEntry, LogQuery } from '../../src/lib/types/logs';
let logFixture: LogEntry[] | undefined;
const makeLogs = (): LogEntry[] => Array.from({length:5000},(_,i)=>({id:i+1,occurredAtUnixMs:1788697000000+i*10,source:i%3?'core':'app',level:i%10?'info':'error',message:`日志 ${i+1} session finished`,fields:{session_id:i+1,stage:'relay',context:{detail:'示例日志字段'.repeat(80)},bytes_up:500,bytes_down:12000}}));
export const getLogs = async (query: LogQuery = {}) => {
  window.dispatchEvent(new CustomEvent('fixture-log-query', {detail:query}));
  logFixture ??= makeLogs();
  const matching=logFixture.filter(e=>(!query.source||e.source===query.source)&&(!query.level||e.level===query.level));
  const items=matching.filter(e=>e.id<(query.beforeId??Infinity)).slice(-(query.limit??400));
  return structuredClone({items,hasMore:!!items.length&&items[0].id>matching[0].id,oldestAvailableId:matching[0]?.id});
};
export const clearLogs = async () => { logFixture = []; };
window.addEventListener('fixture-append-log', () => {
  logFixture ??= makeLogs();
  const id = (logFixture.at(-1)?.id ?? 0) + 1;
  logFixture.push({ id, occurredAtUnixMs: Date.now(), source: 'app', level: 'info', message: `新日志 ${id}`, fields: {stage:'new'} });
});
export const getEffectiveRuleSetOptions = async () => [];
export const getConfigPolicyGroups = async () => [];
export const guiInspectDnsEffectiveConfig = async () => { throw new Error('No kernel in UI fixture'); };
export const getGuiZeroCapabilities = async () => ({ available: false, globalLimitations: [] });
export const guiApplyDnsConfig = async () => { throw new Error('No kernel in UI fixture'); };
export const guiValidateDnsConfig = async () => ({valid: true});
