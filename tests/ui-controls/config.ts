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
export const handleAppError = (_error: unknown, _fallback: string) => {};
import { getTunConfig } from './tun-state.svelte';
export { applyFixtureTun as applyTunSettings } from './tun-state.svelte';
const endpointConfig = () => ({ localProxy: {
  host: '127.0.0.2', port: 8899,
  sourceProxyConfigId: new URLSearchParams(location.search).has('custom') ? 'custom-profile' : null,
} });
let precedenceOverrides = {listener:false,dns:false,tun:false,urlTest:false,bypass:false,rules:false};
export const getAppConfig = async () => {
  const panel = new URLSearchParams(location.search).get('panel');
  if (panel === 'settings' && new URLSearchParams(location.search).get('mode') === 'precedence') {
    return { ...(await getTunConfig()), core: {cleanupProxyOnExit:true}, localProxy: {host:'127.0.0.1',port:7890,bypass:[]},
      overrides:precedenceOverrides, urlTest:{url:'https://client.test/204',toleranceMs:50},
      resolved:[
        {key:'listener',label:'代理入口',source:precedenceOverrides.listener?'客户端显式覆盖':'当前配置',value:precedenceOverrides.listener?'127.0.0.1:7890':'127.0.0.1:7891'},
        {key:'urlTest',label:'公共测速',source:'当前配置',value:'https://source.test/204；2 个策略组另有专用地址'},
        {key:'dns',label:'DNS',source:'当前配置',value:'保留配置中的 DNS 定义'},
        {key:'tun',label:'TUN 参数',source:'客户端缺省值',value:'10.0.85.1/24 · MTU 1500；启停由客户端控制'},
        {key:'bypass',label:'绕过规则',source:'当前配置',value:'保留配置中的绕过规则（含空列表）'},
        {key:'rules',label:'通用规则追加',source:'当前配置',value:'保留配置规则，不追加客户端通用规则'}
      ] };
  }
  if (panel === 'settings' || panel === 'logs') return { ...(await getTunConfig()), core: { autoStart: true, autoConnect: true, cleanupProxyOnExit: true }, ui: { uiMode: 'pro', hiddenMenuKeys: [] }, localProxy: { host: '127.0.0.1', port: 7890, bypass: ['localhost', '127.*'] }, urlTest: { url: 'http://www.gstatic.com/generate_204', toleranceMs: 50 } };
  if (panel === 'url-test') return {urlTest: {url: 'http://www.gstatic.com/generate_204', toleranceMs: 50}};
  if (panel === 'endpoint') return endpointConfig();
  if (panel === 'kernel') return { core: { kernel: 'zero', executablePath: '/fixture/zero', networkProbeUrls: ['https://example.test'] } };
  return getTunConfig();
};
export const getCoreConfigSnapshot = async (): Promise<CoreKernelInfo> => ({ kernel:'zero', executableExists:true, executablePath:'/fixture/zero', recommendedInstallDir:'/fixture', hasActiveConfig:true, warnings:[] });
export const getCoreProcessStatus = async () => ({ state:'running' });
export const getGuiCoreHealth = async () => ({ engineVersion:'0.0.17-rc.1' });
export const updateAppConfig = async (input?: unknown) => {
  if (new URLSearchParams(location.search).get('mode') === 'precedence') {
    precedenceOverrides = {...precedenceOverrides,...(input as {overrides:typeof precedenceOverrides}).overrides};
    window.dispatchEvent(new CustomEvent('fixture-save',{detail:input}));
    return getAppConfig();
  }
  if (['endpoint', 'url-test'].includes(new URLSearchParams(location.search).get('panel') ?? '')) {
    window.dispatchEvent(new CustomEvent('fixture-save', { detail: input }));
    return input;
  }
  window.dispatchEvent(new Event('fixture-unexpected-config-write'));
  throw new Error('Install must not write app settings a second time');
};
export const guiExportDiagnostics = async () => ({ path: 'fixture' });
export const exportClientKernelSettings = async () => {};
export const importClientKernelSettings = async () => {};
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
export const importProxyConfig = async () => structuredClone(profiles[0]);
export const upsertProxyConfig = async () => structuredClone(profiles[0]);
export const removeProxyConfig = async () => {};
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
export const getCoreRuntime = async () => null;
export const getCoreStats = async () => null;
export const guiValidateConfig = async () => ({ valid: true, errors: [] });
export const guiApplyConfig = async () => ({ accepted: true });
export const guiPlanApplyConfig = async () => ({ hotReload: [], requiresRestart: [] });
export const guiApplyDnsConfig = async () => { throw new Error('No kernel in UI fixture'); };
export const guiValidateDnsConfig = async () => ({valid: true});

export const getConfigCompositionReport = async () => null;

// Node browsing and scheduled observations remain usable when manual probes are trimmed.
let selectedNode = 'node-a';
export const guiSelectPolicy = async (_policy: string, tag: string) => {
  selectedNode = tag;
  window.dispatchEvent(new CustomEvent('fixture-save', {detail: {selected: tag}}));
  return {accepted: true};
};
export const getNodeScreenSnapshot = async (): Promise<import('../../src/lib/types/gui-api').NodeScreenSnapshot> => {
  const scope = {profileId: 'fixture', configRevision: 1, coreInstanceId: 1};
  return {revision: 1, scope, sourceStatus: 'ready', activeProbeJobs: [],
    groups: [{id: {profileId:'fixture', configRevision:1, tag:'proxy'}, tag:'proxy', kind:'selector', selected:selectedNode, memberTags:['node-a','node-b'], runtimeAvailable:true, available:true}],
    nodes: ['node-a','node-b'].map((tag,index) => ({id: {profileId:'fixture',configRevision:1,tag},tag, protocol:'vless', groupTags:['proxy'], selectedIn:tag === selectedNode ? ['proxy'] : [], runtimeAvailable:true,alive:true,latencyMs:42+index,lastObservedAtUnixMs:Date.now(),lastObservationSource:'scheduled_policy',activeProbeJobIds:[],actionValid:true,
      history:[{scope,jobKind:'scheduled_policy_observation',targetTag:tag,reachable:true,latencyMs:42+index,source:'scheduled_policy',observedAtUnixMs:Date.now()}]}))};
};

let localFieldEdits: Record<string, unknown> = {};
export const getProfileSettings = async () => {
  const defaults = await getAppConfig();
  const settings = JSON.parse(JSON.stringify(defaults));
  const mode = new URLSearchParams(location.search).get('mode');
  if (mode === 'precedence') settings.localProxy = {host:'127.0.0.2',port:7891,bypass:[]};
  for (const [key,value] of Object.entries(localFieldEdits)) {
    const [section,field] = key.split('.');
    if (field) {settings[section] ??= {}; settings[section][field] = value;}
    else settings[section] = value;
  }
  return {profileId:'fixture-profile',settings,editedFields:Object.keys(localFieldEdits)};
};
export const applyProfileSettings = async (profileId: string, changes: Record<string,unknown>, reset: string[] = []) => {
  if (new URLSearchParams(location.search).get('panel') === 'tun') {
    await new Promise(resolve=>setTimeout(resolve,300));
    if (new URLSearchParams(location.search).get('mode') === 'failure') throw new Error('已恢复旧 TUN 配置');
  }
  if (new URLSearchParams(location.search).get('failure') === 'apply') throw new Error('端口占用，已恢复此前状态');
  for (const key of reset) delete localFieldEdits[key];
  Object.assign(localFieldEdits, changes);
  window.dispatchEvent(new CustomEvent('fixture-save',{detail:{profileId,changes,reset}}));
  return {...(await getProfileSettings()), applied: !new URLSearchParams(location.search).has("stopped")};
};

export const closeFlow = async () => { throw new Error('not available in connection fixture'); };
export const guiCloseConnection = closeFlow;
export async function getGuiDebugFrames(query: import('../../src/lib/types/debug').DebugFrameQuery = {}) {
  const items = Array.from({length: 120}, (_, index) => ({
    id: index + 1, atMs: 1789110000000 + index * 1000, direction: 'rx' as const, frameType: 'event',
    payload: { event_type: 'flow.completed', core_instance_id: 'fixture-kernel', payload: {record: {
      flow_id: String(index + 1), network: 'tcp', target: {host: 'records.test', port: 443},
      timing: {started_at_unix_ms: 1789110000000 + index * 1000, ended_at_unix_ms: 1789110000500 + index * 1000},
      result: {outcome: 'success'},
    }}},
  }));
  const eligible = items.filter(item => item.id < (query.beforeId ?? Infinity));
  return {items: eligible.slice(-(query.limit ?? 50)), hasMore: eligible.length > (query.limit ?? 50), oldestAvailableId: 1,
    history: {retainedRecords: 120, matchedRecords: 120, retainedBytes: 1024,
      oldestCapturedAtMs: items[0].atMs, newestCapturedAtMs: items.at(-1)!.atMs,
      recordLimit: 10000, byteLimit: 33554432, maxAgeMs: 2592000000,
      removedSinceClientStart: 3, writeFailuresSinceClientStart: 0, completeness: 'unknown' as const}};
}
