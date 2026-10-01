import type { ProxyNode } from '$lib/types/protocol';
import type { PolicyGroup } from '$lib/types/gui-api';
import { projectNestedGroupNodes } from '$lib/components/tabs/nodes-view-model';

// Policy groups are real outbound targets, including groups not referenced by
// another group. This is an inventory, never a fabricated selector membership.
export function nodeInventory(nodes: ProxyNode[], groups: PolicyGroup[]): ProxyNode[] {
  const byTag = new Map(nodes.map(node => [node.tag, node]));
  for (const group of groups) {
    const existing = byTag.get(group.name);
    byTag.set(group.name, existing ? { ...existing, protocol: group.kind ?? 'unknown' } : {
      id: `group:${group.name}`, tag: group.name, name: group.name,
      protocol: group.kind ?? 'unknown', delay: 0, domain: 'default',
    });
  }
  return projectNestedGroupNodes([...byTag.values()], groups);
}

export function globalTargetReason(tag: string, nodes: ProxyNode[], groups: PolicyGroup[]): string | null {
  const byGroup = new Map(groups.map(group => [group.name, group]));
  const root = byGroup.get(tag);
  if (root && !root.kind) return '节点组类型未确认';
  if (root?.kind?.toLowerCase() === 'selector' || root?.kind?.toLowerCase() === 'select') {
    return '手动选择组请在左侧独立管理；全局出口选择节点或非 selector 组';
  }
  const known = new Set(nodes.map(node => node.tag));
  const visiting = new Set<string>();
  const visited = new Set<string>();
  function visit(target: string): string | null {
    if (visiting.has(target)) return '节点组存在循环引用，无法作为全局出口';
    const group = byGroup.get(target);
    if (!group) return known.has(target) ? null : `引用的出站 ${target} 不存在`;
    if (visited.has(target)) return null;
    if (!group.outbounds.length) return '节点组没有可用成员';
    visiting.add(target);
    for (const member of group.outbounds) {
      const reason = visit(member.tag);
      if (reason) return reason;
    }
    visiting.delete(target);
    visited.add(target);
    return null;
  }
  return visit(tag);
}
