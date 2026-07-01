import { parseNodeName } from '$lib/services/node-utils';
import type { PolicyGroup, ConfigProxyNode } from '$lib/types/gui-api';
import type { ProxyNode } from '$lib/types/protocol';

export interface RuntimeOverlay {
  delayMs?: number;
  alive?: boolean;
  selected?: boolean;
  groupName?: string;
}

export interface NodeSection {
  name: string;
  kind?: string;
  nodes: ProxyNode[];
}

export function buildRuntimeOverlay(groups: PolicyGroup[]): Map<string, RuntimeOverlay> {
  const map = new Map<string, RuntimeOverlay>();
  for (const group of groups) {
    for (const outbound of group.outbounds) {
      const existing = map.get(outbound.tag);
      map.set(outbound.tag, {
        delayMs: outbound.delayMs ?? existing?.delayMs,
        alive: outbound.alive ?? existing?.alive,
        selected: existing?.selected || group.selected === outbound.tag,
        // Keep the first matching group so the runtime overlay stays
        // consistent with the all-nodes section assignment.
        groupName: existing?.groupName ?? group.name,
      });
    }
  }
  return map;
}

export function buildAllNodes(options: {
  configNodes: ConfigProxyNode[];
  groups: PolicyGroup[];
  runtimeOverlay: Map<string, RuntimeOverlay>;
  latestDelay: (tag: string) => number | undefined;
  latestProbeTime?: (tag: string) => number | undefined;
  fallbackNodes: ProxyNode[];
}): ProxyNode[] {
  const { configNodes, groups, runtimeOverlay, latestDelay, latestProbeTime, fallbackNodes } = options;

  if (configNodes.length > 0) {
    const nodeItems: ProxyNode[] = configNodes.map<ProxyNode>((configNode) => {
      const runtime = runtimeOverlay.get(configNode.tag);
      const parsed = parseNodeName(configNode.tag);
      let delay = runtime?.delayMs ?? latestDelay(configNode.tag) ?? 0;
      // Selector (group) nodes have no own latency — inherit the delay
      // of the group's currently-selected outbound. Switching the
      // selection only re-reads that node's stored delay from history;
      // it never mutates other nodes' entries, so previous measurements
      // remain visible.
      if (configNode.isSelector) {
        const group = groups.find((g) => g.name === configNode.tag);
        const selectedTag = group?.selected;
        if (selectedTag) {
          const selectedRuntime = runtimeOverlay.get(selectedTag);
          delay = selectedRuntime?.delayMs ?? latestDelay(selectedTag) ?? delay;
        }
      }
      return {
        id: configNode.tag,
        tag: configNode.tag,
        name: configNode.tag,
        emoji: parsed.emoji,
        cleanName: parsed.cleanName,
        protocol: configNode.protocol !== 'unknown' ? configNode.protocol : 'proxy',
        delay,
        lastProbeAt: latestProbeTime?.(configNode.tag),
        selected: runtime?.selected,
        alive: runtime?.alive,
        domain: runtime?.groupName ?? 'policy',
        server: configNode.server,
        port: configNode.port,
        udp: configNode.udp,
        network: configNode.network,
        tls: configNode.tls,
        sni: configNode.sni,
        cipher: configNode.cipher,
      };
    });

    // Also surface every policy group as a node. A group nested inside
    // another group (group B listed as a member of group A) then renders as
    // a regular member card inside A — same display, same interaction —
    // instead of being recursively expanded into its leaf nodes. This does
    // not enlarge the default view: the "全部节点" entry is gated to global
    // mode in NodesGroupSidebar, so in rule mode users still pick a specific
    // group and see only its direct members (nodes + nested-group cards).
    // Only render a group as a node when it is nested inside another group
    // (its tag appears in another group's outbounds). Top-level groups stay
    // in the sidebar — clicking a top-level group card has no clear selection
    // semantics and would duplicate the sidebar entries.
    const memberTags = new Set<string>();
    for (const g of groups) {
      for (const o of g.outbounds) memberTags.add(o.tag);
    }
    const existingTags = new Set(nodeItems.map((n) => n.tag));
    const groupItems: ProxyNode[] = [];
    for (const group of groups) {
      if (!memberTags.has(group.name)) continue;
      if (existingTags.has(group.name)) continue;
      const parsed = parseNodeName(group.name);
      let delay = 0;
      if (group.selected) {
        delay = runtimeOverlay.get(group.selected)?.delayMs ?? latestDelay(group.selected) ?? 0;
      }
      groupItems.push({
        id: group.name,
        tag: group.name,
        name: group.name,
        emoji: parsed.emoji,
        cleanName: parsed.cleanName,
        protocol: group.kind || 'group',
        delay,
        lastProbeAt: group.selected ? latestProbeTime?.(group.selected) : undefined,
        selected: runtimeOverlay.get(group.name)?.selected,
        alive: undefined,
        domain: 'policy',
      });
    }

    return [...nodeItems, ...groupItems];
  }

  const seen = new Set<string>();
  const runtimeNodes: ProxyNode[] = [];
  for (const group of groups) {
    for (const outbound of group.outbounds) {
      const key = outbound.tag.toLowerCase();
      if (seen.has(key)) continue;
      seen.add(key);
      const parsed = parseNodeName(outbound.tag);
      runtimeNodes.push({
        id: `${group.name}:${outbound.tag}`,
        tag: outbound.tag,
        name: outbound.tag,
        emoji: parsed.emoji,
        cleanName: parsed.cleanName,
        protocol: outbound.type || 'proxy',
        delay: outbound.delayMs ?? latestDelay(outbound.tag) ?? 0,
        lastProbeAt: latestProbeTime?.(outbound.tag),
        selected: group.selected === outbound.tag,
        alive: outbound.alive,
        domain: group.name,
      });
    }
  }

  return runtimeNodes.length > 0 ? runtimeNodes : fallbackNodes;
}

export function matchesSearch(node: ProxyNode, query: string): boolean {
  if (!query) return true;
  const haystack = `${node.name} ${node.protocol} ${node.server ?? ''} ${node.cleanName ?? ''}`.toLowerCase();
  return haystack.includes(query);
}

export function collectGroupNodeTags(
  groups: PolicyGroup[],
  groupName: string,
): Set<string> {
  // Direct members only — a nested group stays as a member tag rather than
  // being recursively expanded into its leaves, so it can render as a member
  // card alongside regular nodes (see buildAllNodes).
  const group = groups.find((item) => item.name === groupName);
  if (!group) return new Set();
  return new Set(group.outbounds.map((outbound) => outbound.tag));
}

export function filterNodes(options: {
  allNodes: ProxyNode[];
  groups: PolicyGroup[];
  query: string;
  selectedGroup: string | null;
}): ProxyNode[] {
  const { allNodes, groups, query, selectedGroup } = options;
  let nodes = allNodes.filter((node) => matchesSearch(node, query));

  if (selectedGroup) {
    const group = groups.find((g) => g.name === selectedGroup);
    if (group) {
      // Render members in the order declared in the group's outbounds, so a
      // nested group listed first stays first — instead of being pushed after
      // regular nodes by the allNodes ordering (which appends group cards at
      // the tail).
      const nodeMap = new Map(nodes.map((n) => [n.tag, n]));
      const ordered = group.outbounds
        .map((o) => o.tag)
        .map((tag) => nodeMap.get(tag))
        .filter((n): n is ProxyNode => n !== undefined);
      if (ordered.length > 0) nodes = ordered;
    }
  }

  return nodes;
}

export function buildSections(options: {
  allNodes: ProxyNode[];
  groups: PolicyGroup[];
  query: string;
  orphanSectionName?: string;
}): NodeSection[] {
  const { allNodes, groups, query, orphanSectionName = '其他' } = options;
  const filtered = allNodes.filter((node) => matchesSearch(node, query));
  if (filtered.length === 0) return [];

  const tagToGroup = new Map<string, string>();
  for (const group of groups) {
    for (const outbound of group.outbounds) {
      if (!tagToGroup.has(outbound.tag)) tagToGroup.set(outbound.tag, group.name);
    }
  }

  const buckets = new Map<string, ProxyNode[]>();
  const orphan: ProxyNode[] = [];

  for (const node of filtered) {
    const groupName = tagToGroup.get(node.tag);
    if (groupName) {
      if (!buckets.has(groupName)) buckets.set(groupName, []);
      buckets.get(groupName)!.push(node);
    } else {
      orphan.push(node);
    }
  }

  const sections: NodeSection[] = [];
  for (const group of groups) {
    const items = buckets.get(group.name);
    if (items && items.length > 0) {
      sections.push({ name: group.name, kind: group.kind, nodes: items });
    }
  }

  if (orphan.length > 0) {
    sections.push({ name: orphanSectionName, nodes: orphan });
  }

  return sections;
}

export function getActiveNodeTag(groups: PolicyGroup[]): string | undefined {
  for (const group of groups) {
    if (group.selected) return group.selected;
  }
  return undefined;
}

export function normalizeSelectedGroup(
  selectedGroup: string | null,
  groups: PolicyGroup[],
): string | null {
  if (!selectedGroup) return null;
  return groups.some((group) => group.name === selectedGroup) ? selectedGroup : null;
}

export function resolveNodeGroup(options: {
  groups: PolicyGroup[];
  runtimeOverlay: Map<string, RuntimeOverlay>;
  selectedGroup: string | null;
  nodeTag: string;
}): PolicyGroup | undefined {
  const { groups, runtimeOverlay, selectedGroup, nodeTag } = options;
  const byName = (name: string | null | undefined) =>
    name ? groups.find((group) => group.name === name) : undefined;

  return (
    byName(selectedGroup) ??
    byName(runtimeOverlay.get(nodeTag)?.groupName) ??
    groups.find((group) => group.outbounds.some((outbound) => outbound.tag === nodeTag))
  );
}

export function isSelectableGroup(group?: PolicyGroup): boolean {
  if (!group) return true;
  return group.kind?.toLowerCase() === 'selector';
}
