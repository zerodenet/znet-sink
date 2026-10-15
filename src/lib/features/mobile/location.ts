/** Read-only host identity. An opaque binding is not a control permission. */
export type MobileLocationBinding = Readonly<{
  binding: string;
  profileId: string;
  configRevision: string;
  coreInstanceId: string;
  scopeGeneration: string;
}>;

export type MobileLocationTarget =
  | Readonly<{ kind: 'profile'; profileId: string | null; binding: MobileLocationBinding | null }>
  | Readonly<{ kind: 'nodes'; binding: MobileLocationBinding; selection:
      Readonly<{ kind: 'global'; rootTag: string }> | Readonly<{ kind: 'rule' }> | Readonly<{ kind: 'direct' }> | Readonly<{ kind: 'unconfirmed' }> }>;

export interface MobileLocationSnapshot {
  binding: MobileLocationBinding | null;
  profile: { id: string; name: string } | null;
  reason: string | null;
}

/** Reject lossy legacy numbers before comparing native identities. */
export function locationDecimal(value: unknown): string | null {
  if (typeof value === 'number') return Number.isSafeInteger(value) && value >= 0 ? String(value) : null;
  return typeof value === 'string' && value.length <= 20 && /^(0|[1-9][0-9]*)$/.test(value)
    && BigInt(value) <= 18446744073709551615n ? value : null;
}

export function validLocationSnapshot(value: MobileLocationSnapshot): boolean {
  if (!value || !(value.reason === null || typeof value.reason === 'string')) return false;
  const b = value.binding;
  if (!b) return b === null && value.profile === null;
  return !!b.binding && typeof b.binding === 'string' && typeof b.profileId === 'string' && !!b.profileId
    && !!value.profile && value.profile.id === b.profileId && typeof value.profile.name === 'string'
    && [b.configRevision, b.coreInstanceId, b.scopeGeneration].every(v => typeof v === 'string' && locationDecimal(v) !== null)
    && b.scopeGeneration !== '0';
}

export function sameLocationBinding(a: MobileLocationBinding | null, b: MobileLocationBinding | null): boolean {
  return a !== null && b !== null && a.binding === b.binding && a.profileId === b.profileId
    && a.configRevision === b.configRevision && a.coreInstanceId === b.coreInstanceId && a.scopeGeneration === b.scopeGeneration;
}

export function sameLocationRead(a: MobileLocationSnapshot, b: MobileLocationSnapshot): boolean {
  return validLocationSnapshot(a) && validLocationSnapshot(b)
    && (a.binding === null && b.binding === null ? a.profile === null && b.profile === null : sameLocationBinding(a.binding, b.binding));
}

export function locationScopeMatches(binding: MobileLocationBinding, scope: import('$lib/types/gui-api').ClientScope): boolean {
  return !!scope && scope.profileId === binding.profileId && locationDecimal(scope.configRevision) === binding.configRevision
    && locationDecimal(scope.coreInstanceId) === binding.coreInstanceId;
}

export function locationSnapshotMatches(target: MobileLocationTarget, snapshot: MobileLocationSnapshot): boolean {
  if (!validLocationSnapshot(snapshot)) return false;
  if (target.kind === 'profile' && target.profileId === null) return snapshot.profile === null && snapshot.binding === null;
  return sameLocationBinding(target.binding, snapshot.binding)
    && (target.kind !== 'profile' || target.profileId === snapshot.profile?.id);
}

export interface LocatedNodeChain { tags: string[]; groups: string[]; leafTag: string | null; reason: string | null }

/** Resolve only declared, same-scope members. Relay/order and unknown winners stay unknown. */
export function locateNodeChain(snapshot: import('$lib/types/gui-api').NodeScreenSnapshot, binding: MobileLocationBinding, rootTag: string): LocatedNodeChain {
  const unknown = (reason: string, tags: string[] = [], groups: string[] = []): LocatedNodeChain => ({ tags, groups, leafTag: null, reason });
  if (!snapshot || !locationScopeMatches(binding, snapshot.scope)) return unknown('节点快照不属于原定位配置或实例。');
  if (!Array.isArray(snapshot.groups) || !Array.isArray(snapshot.nodes)) return unknown('节点目录结构未提供，无法安全定位。');
  const ids = [...snapshot.groups, ...snapshot.nodes];
  if (ids.some(item => !item.id || item.id.profileId !== binding.profileId || locationDecimal(item.id.configRevision) !== binding.configRevision || item.id.tag !== item.tag)) return unknown('节点资源身份与当前配置不一致。');
  if (new Set(snapshot.groups.map(g => g.tag)).size !== snapshot.groups.length || new Set(snapshot.nodes.map(n => n.tag)).size !== snapshot.nodes.length) return unknown('节点资源标签重复，无法安全定位。');
  const groups = new Map(snapshot.groups.map(g => [g.tag, g]));
  const leaves = new Map(snapshot.nodes.map(n => [n.tag,n]));
  const path: string[] = [];
  const chain: string[] = [];
  let tag = rootTag;
  while (tag) {
    if (chain.includes(tag)) return unknown('出口组存在循环，最终节点未确认。', chain, path);
    const group = groups.get(tag);
    if (!group) {
      const leaf=leaves.get(tag);
      if(!leaf)return unknown('原出口已移除或未提供，未定位到同名替代对象。',chain,path);
      const protocol=typeof leaf.protocol==='string' ? leaf.protocol.toLowerCase().replace(/[-_]/g,'') : '';
      if(['selector','select','urltest','fallback','relay'].includes(protocol))return unknown('组的成员目录未提供；不能把组卡片当作最终节点。',[...chain,tag],path);
      if(leaf.runtimeAvailable!==true)return unknown('此节点只有配置资料，运行态未确认。',[...chain,tag],path);
      return {tags:[...chain,tag],groups:path,leafTag:tag,reason:null};
    }
    chain.push(tag); path.push(tag);
    const kind = typeof group.kind === 'string' ? group.kind.toLowerCase().replace(/[-_]/g, '') : '';
    if (!['selector', 'select', 'urltest', 'fallback'].includes(kind)) return unknown('此组按配置顺序或专有语义执行，未提供唯一最终节点。', chain, path);
    if (group.runtimeAvailable!==true || !group.selected || !Array.isArray(group.memberTags) || !group.memberTags.includes(group.selected)) return unknown('组的实际有效成员未确认。', chain, path);
    tag = group.selected;
  }
  return unknown('出口身份未提供。', chain, path);
}
