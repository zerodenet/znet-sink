// In-memory fixture only: the UI suite must never call Tauri, DNS or a kernel.
import type { CommonRuleBindingInput, RuleSetSummary, RuleSetUpsert } from '../../src/lib/types/domain';

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
export const getAppErrorMessage = (_error: unknown, fallback: string) => fallback;
