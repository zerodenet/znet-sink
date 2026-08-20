import type { PolicyGroup, ProbeJobSnapshot } from '$lib/types/gui-api';
import type { ProxyNode } from '$lib/types/protocol';
import { matchesNodeHealthFilter } from '$lib/components/tabs/nodes-display-preferences.svelte';

export interface ProbeTargets {
  nodes: ProxyNode[];
}

export interface NodeSection {
  name: string;
  kind?: string;
  nodes: ProxyNode[];
}

/**
 * Build one outbound diagnostic target for every visible node card.
 *
 * Nested policy groups are effective outbounds in their parent group, so a
 * URLTest card probes its currently selected route instead of implicitly
 * starting a full policy refresh. Built-in outbounds such as direct and reject
 * follow the same path and let the kernel report success or failure.
 */
export function planProbeTargets(options: {
  groups: PolicyGroup[];
  selectedGroup: string | null;
  visibleNodes: ProxyNode[];
}): ProbeTargets {
  const nodeTags = new Set<string>();
  const nodes: ProxyNode[] = [];

  for (const node of options.visibleNodes) {
    if (nodeTags.has(node.tag)) continue;
    nodeTags.add(node.tag);
    nodes.push(node);
  }

  return { nodes };
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

function isUrlTestGroup(group: PolicyGroup): boolean {
  return group.kind?.toLowerCase().replace(/[-_]/g, '') === 'urltest';
}

function urlTestLatencyRank(node: ProxyNode): number {
  const failed = node.delay < 0 || (node.alive === false && node.lastProbeAt !== undefined);
  if (failed) return 2;
  if (node.delay > 0) return 0;
  return 1;
}

/**
 * URLTest groups present their successful observations from fastest to slowest.
 * Nodes without an observation retain their configured order between successful
 * and failed members, while timeout/failure observations remain at the end.
 */
function sortGroupNodes(group: PolicyGroup, nodes: ProxyNode[]): ProxyNode[] {
  if (!isUrlTestGroup(group)) return nodes;

  return nodes
    .map((node, index) => ({ node, index }))
    .sort((a, b) => {
      const rankDiff = urlTestLatencyRank(a.node) - urlTestLatencyRank(b.node);
      if (rankDiff !== 0) return rankDiff;
      if (urlTestLatencyRank(a.node) === 0) {
        const delayDiff = a.node.delay - b.node.delay;
        if (delayDiff !== 0) return delayDiff;
      }
      return a.index - b.index;
    })
    .map(({ node }) => node);
}

/**
 * A nested policy group is rendered as one effective outbound card in its
 * parent. Runtime parent-member metadata can lag behind the nested group's own
 * selection after a scheduled/manual URLTest cycle, so derive that card's
 * volatile probe state from the nested group's final selected leaf.
 *
 * The projection is deliberately presentation-only: identity, protocol and
 * parent selection flags stay on the group card. Only latency/health/time are
 * borrowed from the effective selected route. Missing fresh state clears the
 * stale previous-member values instead of keeping them.
 */
export function projectNestedGroupNodes(
  allNodes: ProxyNode[],
  groups: PolicyGroup[],
): ProxyNode[] {
  const nodesByTag = new Map(allNodes.map((node) => [node.tag, node]));
  const groupsByName = new Map(groups.map((group) => [group.name, group]));

  const resolveEffectiveNode = (
    tag: string,
    visiting: Set<string>,
  ): ProxyNode | undefined => {
    if (visiting.has(tag)) return nodesByTag.get(tag);
    const group = groupsByName.get(tag);
    if (!group?.selected) return nodesByTag.get(tag);

    visiting.add(tag);
    const resolved = resolveEffectiveNode(group.selected, visiting)
      ?? nodesByTag.get(group.selected);
    visiting.delete(tag);
    return resolved;
  };

  return allNodes.map((node) => {
    if (!groupsByName.has(node.tag)) return node;
    const effective = resolveEffectiveNode(node.tag, new Set<string>());
    if (!effective || effective.tag === node.tag) return node;

    return {
      ...node,
      delay: effective.delay,
      lastProbeAt: effective.lastProbeAt,
      alive: effective.alive,
    };
  });
}

export function filterNodes(options: {
  allNodes: ProxyNode[];
  groups: PolicyGroup[];
  query: string;
  selectedGroup: string | null;
}): ProxyNode[] {
  const { allNodes, groups, query, selectedGroup } = options;
  const projected = projectNestedGroupNodes(allNodes, groups);
  const nodes = projected.filter((node) => matchesSearch(node, query) && matchesNodeHealthFilter(node));
  if (!selectedGroup) return nodes;
  const group = groups.find((item) => item.name === selectedGroup);
  if (!group) return nodes;
  const byTag = new Map(nodes.map((node) => [node.tag, node]));
  const groupNodes = group.outbounds
    .map((outbound) => byTag.get(outbound.tag))
    .filter((node): node is ProxyNode => node !== undefined);
  return sortGroupNodes(group, groupNodes);
}

export function buildSections(options: {
  allNodes: ProxyNode[];
  groups: PolicyGroup[];
  query: string;
  orphanSectionName?: string;
}): NodeSection[] {
  const { allNodes, groups, query, orphanSectionName = '其他' } = options;
  const projected = projectNestedGroupNodes(allNodes, groups);
  const filtered = projected.filter((node) => matchesSearch(node, query) && matchesNodeHealthFilter(node));
  const assigned = new Set<string>();
  const sections: NodeSection[] = groups.flatMap((group) => {
    const byTag = new Map(filtered.map((node) => [node.tag, node]));
    const nodes = group.outbounds
      .map((outbound) => byTag.get(outbound.tag))
      .filter((node): node is ProxyNode => node !== undefined && !assigned.has(node.id));
    for (const node of nodes) assigned.add(node.id);
    return nodes.length > 0
      ? [{ name: group.name, kind: group.kind, nodes: sortGroupNodes(group, nodes) }]
      : [];
  });
  const orphan = filtered.filter((node) => !assigned.has(node.id));
  if (orphan.length > 0) sections.push({ name: orphanSectionName, nodes: orphan });
  return sections;
}

export interface EffectiveNodeSelection {
  /** Final non-policy outbound selected by the runtime chain. */
  leafTag?: string;
  /** Policy groups traversed from the configured root to the leaf. */
  groupPath: string[];
  /** Kind of the policy group that directly selected the leaf. */
  leafParentKind?: string;
  /** Group whose runtime selection is missing or cyclic. */
  unresolvedGroupTag?: string;
  cycleDetected: boolean;
}

/**
 * Resolve a configured outbound through any number of nested policy groups.
 *
 * `selected` is runtime state for selector / URLTest / fallback-style groups;
 * following that chain is the only truthful way for compact UI to name the
 * actual leaf outbound. Never infer an automatic group's winner from latency.
 */
export function resolveEffectiveNodeSelection(
  groups: PolicyGroup[],
  rootTag: string | null | undefined,
): EffectiveNodeSelection {
  if (!rootTag) return { groupPath: [], cycleDetected: false };

  const groupsByName = new Map(groups.map((group) => [group.name, group]));
  const visiting = new Set<string>();
  const groupPath: string[] = [];
  let currentTag = rootTag;
  let leafParentKind: string | undefined;

  while (currentTag) {
    const group = groupsByName.get(currentTag);
    if (!group) {
      return {
        leafTag: currentTag,
        groupPath,
        leafParentKind,
        cycleDetected: false,
      };
    }

    if (visiting.has(currentTag)) {
      return {
        groupPath,
        leafParentKind,
        unresolvedGroupTag: currentTag,
        cycleDetected: true,
      };
    }

    visiting.add(currentTag);
    groupPath.push(currentTag);
    leafParentKind = group.kind;

    if (!group.selected) {
      return {
        groupPath,
        leafParentKind,
        unresolvedGroupTag: currentTag,
        cycleDetected: false,
      };
    }

    currentTag = group.selected;
  }

  return { groupPath, leafParentKind, cycleDetected: false };
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
