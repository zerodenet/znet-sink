import type { ConfigWorkspaceSnapshot, ConfigWorkspaceDraftInput, ConfigWorkspacePlan, ConfigWorkspaceOperation } from '../../src/lib/types/gui-api';
import { proxyConfigSignal } from '../../src/lib/services/proxy-config-signal.svelte';
export const nativeText = '{\n  "route": {"final": {"type": "direct"}},\n  "runtime": {"udp_upstream_idle_timeout_seconds": 18446744073709551615}\n}';
let version = 1, serial = 0;
let current: ConfigWorkspaceSnapshot = {
  profileId: 'main', profileName: '日常网络配置', binding: 'source-1', sourceUpdatedAtUnixMs: '1', sourceJson: nativeText,
  localEditsJson: '{"localProxy.port":7890}', effectiveJson: nativeText, pendingApply: true,
  composition: { layers: [{ source: 'client', changedPaths: ['/inbounds'] }] },
  runtime: { running: false, effectiveDigest: 'fixture', confirmed: false, reason: '内核未运行，已保存的本地材料待应用' },
};
const plans = new Map<string, ConfigWorkspacePlan>();
const operations = new Map<string, ConfigWorkspaceOperation>();
const calls: {kind: string; input?: unknown}[] = [];
export const workspaceFixture = {
  online: false, loseApply: false, failSave: false, holdPlan: false, holdSave: false,
  releaseSave: (() => {}) as () => void,
  releasePlan: (() => {}) as () => void,
  calls,
  changeSource(managed = false) {
    version++;
    current = { ...current, binding: managed ? null : `source-${version}`, sourceUpdatedAtUnixMs: '1',
      sourceJson: managed ? null : nativeText.replace('"direct"', '"reject"'),
      localEditsJson: managed ? null : '{}', effectiveJson: managed ? null : nativeText,
      readOnlyReason: managed ? '此配置由插件托管，普通工作区不读取正文；请到来源插件管理' : null };
    proxyConfigSignal.markChanged(true);
  },
};
Object.assign(window, { workspaceFixture });
export async function getConfigWorkspaceSnapshot(): Promise<ConfigWorkspaceSnapshot> { calls.push({kind:'snapshot'}); return structuredClone(current); }
export async function validateConfigWorkspace(input: ConfigWorkspaceDraftInput) {
  calls.push({kind:'validate',input});
  if (input.binding !== current.binding || current.readOnlyReason) throw new Error('来源已变化，请刷新');
  if (!input.sourceJson.includes('"route"')) throw { message:'Zero原生校验失败，草稿保留', details:{errors:[{field_path:'route',message:'缺少原生路由字段'}]} };
  return {sourceJson:input.sourceJson,effectiveJson:input.sourceJson};
}
export async function formatConfigWorkspace(sourceJson: string): Promise<string> {
  calls.push({kind:'format',input:sourceJson});
  // IO fixture preserves original numeric lexemes. Actual Rust formatting is covered in native tests.
  return sourceJson.trim();
}
export async function planConfigWorkspace(input: ConfigWorkspaceDraftInput): Promise<ConfigWorkspacePlan> {
  calls.push({kind:'plan',input});
  if (workspaceFixture.holdPlan) await new Promise<void>(resolve => workspaceFixture.releasePlan=resolve);
  await validateConfigWorkspace(input);
  const plan: ConfigWorkspacePlan = { valid:true,hotReload:[],requiresRestart:[],warnings:[],errors:[],planId:`plan-${++serial}`,binding:input.binding,sourceJson:input.sourceJson,effectiveJson:input.sourceJson,effectiveDigest:'same-digest',expiresAtUnixMs:'9999999999999',runtimeValidated:workspaceFixture.online,applyUnavailableReason:workspaceFixture.online ? null : '内核未运行；可保存本地待应用，未确认运行兼容' };
  plans.set(plan.planId,plan); return plan;
}
export async function discardConfigWorkspacePlan(planId: string): Promise<void> { calls.push({kind:'discard',input:planId}); plans.delete(planId); }
export async function saveConfigWorkspaceLocal(input: ConfigWorkspaceDraftInput) {
  calls.push({kind:'save',input});
  if (workspaceFixture.holdSave) await new Promise<void>(resolve => workspaceFixture.releaseSave=resolve);
  if (workspaceFixture.failSave) throw new Error('本地存储失败；原草稿保留');
  await validateConfigWorkspace(input); version++;
  current = {...current,binding:`source-${version}`,sourceUpdatedAtUnixMs:String(version),sourceJson:input.sourceJson,effectiveJson:input.sourceJson,pendingApply:true};
  return {state:'saved_pending_apply' as const,snapshot:structuredClone(current)};
}
export async function applyConfigWorkspace(input: {planId: string; strategy:'hot_reload'|'restart'}): Promise<ConfigWorkspaceOperation> {
  calls.push({kind:'apply',input});
  const plan=plans.get(input.planId); if (!plan) throw new Error('原计划已消费'); plans.delete(input.planId);
  const operation: ConfigWorkspaceOperation = {planId:input.planId,state:'submitted_unknown',reason:'原操作未确认；不会重放'};
  operations.set(input.planId,operation);
  if (workspaceFixture.loseApply) throw new Error('回执丢失');
  return operation;
}
export async function inspectConfigWorkspaceOperation(planId: string): Promise<ConfigWorkspaceOperation> {
  calls.push({kind:'operation',input:planId}); return operations.get(planId) ?? {planId,state:'submitted_unknown',reason:'原事务仍未确认'};
}
