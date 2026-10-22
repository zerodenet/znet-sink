import type { ClientScope } from '$lib/types/gui-api';

export function sameProbeScope(a: ClientScope, b: ClientScope): boolean {
  return a.profileId === b.profileId && String(a.configRevision) === String(b.configRevision) && a.coreInstanceId === b.coreInstanceId;
}
/** Old unsafe numeric identities must never be rounded and sent back as control preconditions. */
export function controlScope(scope: ClientScope): ClientScope {
  const revision=String(scope.configRevision);
  if ((typeof scope.configRevision==='number' && !Number.isSafeInteger(scope.configRevision)) ||
      !/^(0|[1-9][0-9]*)$/.test(revision) || BigInt(revision)>18446744073709551615n ||
      !Number.isSafeInteger(scope.coreInstanceId) || scope.coreInstanceId<0) {
    throw new Error('所属配置或实例标识超出安全范围，请刷新客户端状态');
  }
  return {...scope,configRevision:revision};
}
