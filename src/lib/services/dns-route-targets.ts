import type { ConfigProxyNode, PolicyGroup } from '$lib/types/gui-api';

export interface DnsRouteTargetOption {
  tag: string;
  label: string;
}

/** DNS detours can name concrete outbounds as well as policy groups. */
export function dnsRouteTargetOptions(
  nodes: ReadonlyArray<Pick<ConfigProxyNode, 'tag' | 'protocol'>>,
  groups: ReadonlyArray<Pick<PolicyGroup, 'name'>>,
): DnsRouteTargetOption[] {
  const targets = new Map<string, DnsRouteTargetOption>([
    ['direct', { tag: 'direct', label: '直接连接' }],
    ['block', { tag: 'block', label: '阻断' }],
  ]);
  for (const node of nodes) {
    if (!node.tag || targets.has(node.tag)) continue;
    targets.set(node.tag, { tag: node.tag, label: `出站 · ${node.tag} (${node.protocol})` });
  }
  for (const group of groups) {
    if (!group.name || targets.has(group.name)) continue;
    targets.set(group.name, { tag: group.name, label: `策略组 · ${group.name}` });
  }
  return [...targets.values()].sort((left, right) => left.tag.localeCompare(right.tag, 'zh-CN'));
}
