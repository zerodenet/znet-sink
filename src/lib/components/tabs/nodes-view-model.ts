import type { PolicyGroup, ProbeJobSnapshot } from '$lib/types/gui-api';
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

/**
 * Build probe targets from the cards that are currently visible to the user.
 *
 * A normal card is one outbound probe target. A nested URLTest group is also
 * one visible card, so it becomes one policy probe target. We intentionally do
 * not collapse a selected URLTest group into a single policy job: once the user
 * opens that group, its direct members are the effective visible targets and
 * progress must reflect those cards individually.
 */
export function planProbeTargets(options: {
  groups: PolicyGroup[];
  selectedGroup: string | null;
  visibleNodes: ProxyNode[];
}): ProbeTargets {
  const { groups, visibleNodes } = options;
  const groupsByName = new Map(groups.map((group) => [group.name, group]));
  const policyTags = new Set<string>();
  const nodeTags = new Set<string>();
  const nodes: ProxyNode[] = [];

  for (const node of visibleNodes) {
    if (isSpecialOutboundProtocol(node.protocol)) continue;

    const nestedGroup = groupsByName.get(node.tag);
    if (isUrlTestGroup(nestedGroup)) {
      policyTags.add(nestedGroup!.name);
      continue;
    }

    if (nodeTags.has(node.tag)) continue;
    nodeTags.add(node.tag);
    nodes.push(node);
  }

  return { nodes, policyTags: [...policyTags] };
}

/**
 * A policy probe belongs to the nested group card that started it. Its member
 * cards may receive runtime observations later, but they must not all enter the
 * manual loading state for a single card action.
 */
export function collectProbingPolicyNodeTags(
  groups: PolicyGroup[],
  probingPolicyTags: ReadonlySet<string>,
): Set<string> {
  const tags = new Set<string>();
  for (const group of groups) {
    if (probingPolicyTags.has(group.name)) tags.add(group.name);
  }
  return tags;
}

export interface ProbeProgress {
  done: number;
  total: number;
}

function policyProbeLeafTags(
  groupsByName: ReadonlyMap<string, PolicyGroup>,
  policyTag: string,
): Set<string> {
  const leaves = new Set<string>();
  const visiting = new Set<string>();

  const visit = (tag: string) => {
    const group = groupsByName.get(tag);
    if (!group) {
      leaves.add(tag);
      return;
    }
    if (visiting.has(tag)) return;
    visiting.add(tag);
    for (const member of group.outbounds) visit(member.tag);
    visiting.delete(tag);
  };

  visit(policyTag);
  if (leaves.size === 0) leaves.add(policyTag);
  return leaves;
}

/**
 * Project active Client Core jobs into the number of effective node probes.
 * Policy jobs carry a policy tag as their job target, so expand that tag through
 * nested groups and de-duplicate leaf nodes before displaying progress. The
 * current kernel only returns policy completion as a whole, therefore policy
 * leaves move from 0/N to N/N together when the policy result arrives.
 */
export function summarizeProbeProgress(
  groups: PolicyGroup[],
  jobs: ProbeJobSnapshot[],
): ProbeProgress {
  const groupsByName = new Map(groups.map((group) => [group.name, group]));
  const requested = new Set<string>();
  const completed = new Set<string>();

  for (const job of jobs) {
    if (job.kind === 'outbound') {
      for (const targetTag of job.targetTags) requested.add(targetTag);
      for (const result of job.results) completed.add(result.targetTag);
      continue;
    }
    if (job.kind !== 'manual_policy') continue;

    for (const policyTag of job.targetTags) {
      const leaves = policyProbeLeafTags(groupsByName, policyTag);
      for (const leaf of leaves) requested.add(leaf);
      if (job.results.some((result) => result.targetTag === policyTag)) {
        for (const leaf of leaves) completed.add(leaf);
      }
    }
  }

  let done = 0;
  for (const tag of requested) {
    if (completed.has(tag)) done += 1;
  }
  return { done, total: requested.size };
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
