import type { PolicyGroup } from '$lib/types/gui-api';
import type { ProxyNode } from '$lib/types/protocol';
import { isSpecialOutboundProtocol } from '$lib/services/node-utils';

export interface ProbeTargets {
  nodes: ProxyNode[];
  policyTags: string[];
}

export interface NodeSection {
  name: string;
  kind?: string;
  nodes: ProxyNode[];
}

function isUrlTestGroup(group: PolicyGroup | undefined): boolean {
  const kind = group?.kind?.toLowerCase();
  return kind === 'url_test' || kind === 'urltest';
}

export function policyProbeTagForNode(
  groups: PolicyGroup[],
  nodeTag: string,
): string | undefined {
  const group = groups.find((item) => item.name === nodeTag);
  return isUrlTestGroup(group) ? group?.name : undefined;
}

function collectDescendantProbeTags(
  groupsByName: Map<string, PolicyGroup>,
  groupTag: string,
  visited = new Set<string>(),
): Set<string> {
  if (visited.has(groupTag)) return new Set();
  visited.add(groupTag);
  const group = groupsByName.get(groupTag);
  if (!group) return new Set();
  const tags = new Set<string>();
  for (const member of group.outbounds) {
    tags.add(member.tag);
    if (groupsByName.has(member.tag)) {
      for (const nested of collectDescendantProbeTags(groupsByName, member.tag, visited)) {
        tags.add(nested);
      }
    }
  }
  return tags;
}

/** UI intent projection only. Node/group state itself is supplied by Rust. */
export function planProbeTargets(options: {
  groups: PolicyGroup[];
  selectedGroup: string | null;
  visibleNodes: ProxyNode[];
}): ProbeTargets {
  const { groups, selectedGroup, visibleNodes } = options;
  const selected = groups.find((group) => group.name === selectedGroup);
  if (isUrlTestGroup(selected)) return { nodes: [], policyTags: [selected!.name] };

  const groupsByName = new Map(groups.map((group) => [group.name, group]));
  const policyTags = new Set<string>();
  for (const node of visibleNodes) {
    const memberGroup = groupsByName.get(node.tag);
    if (isUrlTestGroup(memberGroup)) policyTags.add(memberGroup!.name);
  }
  const policyOwnedTags = new Set<string>();
  for (const policyTag of policyTags) {
    for (const tag of collectDescendantProbeTags(groupsByName, policyTag)) {
      policyOwnedTags.add(tag);
    }
  }

  const seen = new Set<string>();
  const nodes = visibleNodes.filter((node) => {
    if (isSpecialOutboundProtocol(node.protocol)) return false;
    if (isUrlTestGroup(groupsByName.get(node.tag))) return false;
    if (policyOwnedTags.has(node.tag) || seen.has(node.tag)) return false;
    seen.add(node.tag);
    return true;
  });
  return { nodes, policyTags: [...policyTags] };
}

export function collectProbingPolicyNodeTags(
  groups: PolicyGroup[],
  probingPolicyTags: ReadonlySet<string>,
): Set<string> {
  const tags = new Set<string>();
  for (const group of groups) {
    if (!probingPolicyTags.has(group.name)) continue;
    tags.add(group.name);
    for (const member of group.outbounds) tags.add(member.tag);
  }
  return tags;
}

export function matchesSearch(node: ProxyNode, query: string): boolean {
  if (!query) return true;
  return `${node.name} ${node.protocol} ${node.server ?? ''} ${node.cleanName ?? ''}`
    .toLowerCase()
    .includes(query);
}

export function collectGroupNodeTags(groups: PolicyGroup[], groupName: string): Set<string> {
  const group = groups.find((item) => item.name === groupName);
  return new Set(group?.outbounds.map((outbound) => outbound.tag) ?? []);
}

export function filterNodes(options: {
  allNodes: ProxyNode[];
  groups: PolicyGroup[];
  query: string;
  selectedGroup: string | null;
}): ProxyNode[] {
  const { allNodes, groups, query, selectedGroup } = options;
  const nodes = allNodes.filter((node) => matchesSearch(node, query));
  if (!selectedGroup) return nodes;
  const group = groups.find((item) => item.name === selectedGroup);
  if (!group) return nodes;
  const byTag = new Map(nodes.map((node) => [node.tag, node]));
  return group.outbounds
    .map((outbound) => byTag.get(outbound.tag))
    .filter((node): node is ProxyNode => node !== undefined);
}

export function buildSections(options: {
  allNodes: ProxyNode[];
  groups: PolicyGroup[];
  query: string;
  orphanSectionName?: string;
}): NodeSection[] {
  const { allNodes, groups, query, orphanSectionName = '其他' } = options;
  const filtered = allNodes.filter((node) => matchesSearch(node, query));
  const assigned = new Set<string>();
  const sections: NodeSection[] = groups.flatMap((group) => {
    const byTag = new Map(filtered.map((node) => [node.tag, node]));
    const nodes = group.outbounds
      .map((outbound) => byTag.get(outbound.tag))
      .filter((node): node is ProxyNode => node !== undefined && !assigned.has(node.id));
    for (const node of nodes) assigned.add(node.id);
    return nodes.length > 0 ? [{ name: group.name, kind: group.kind, nodes }] : [];
  });
  const orphan = filtered.filter((node) => !assigned.has(node.id));
  if (orphan.length > 0) sections.push({ name: orphanSectionName, nodes: orphan });
  return sections;
}

export function getActiveNodeTag(
  groups: PolicyGroup[],
  selectedGroup: string | null = null,
): string | undefined {
  if (selectedGroup) return groups.find((group) => group.name === selectedGroup)?.selected;
  return groups.find((group) => group.selected)?.selected;
}

export function normalizeSelectedGroup(
  selectedGroup: string | null,
  groups: PolicyGroup[],
): string | null {
  if (!selectedGroup) return null;
  return groups.some((group) => group.name === selectedGroup) ? selectedGroup : null;
}

export function isSelectableGroup(group?: PolicyGroup): boolean {
  return !group || group.kind?.toLowerCase() === 'selector';
}
