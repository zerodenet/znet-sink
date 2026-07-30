import type { PolicyGroup, PolicyProbeCompletedEvent } from '$lib/types/gui-api';

export interface PolicyProbeHistoryUpdate {
  tag: string;
  delayMs?: number;
  reachable: boolean;
  at: number;
  selectedTag?: string;
}

/**
 * Record one concrete probe target and project that observation through every
 * policy group whose current selection points at it. This also supports nested
 * groups and stops safely when a malformed configuration contains a cycle.
 */
export function buildProbeHistoryUpdates(
  groups: PolicyGroup[],
  targetTag: string,
  delayMs: number | undefined,
  reachable: boolean,
  at = Date.now(),
): PolicyProbeHistoryUpdate[] {
  if (!targetTag) return [];

  const groupsByName = new Map(groups.map((group) => [group.name, group]));
  const updates: PolicyProbeHistoryUpdate[] = [{
    tag: targetTag,
    delayMs,
    reachable,
    at,
    ...(groupsByName.get(targetTag)?.selected
      ? { selectedTag: groupsByName.get(targetTag)?.selected }
      : {}),
  }];
  const visited = new Set([targetTag]);
  const queue = [targetTag];

  while (queue.length > 0) {
    const selectedTag = queue.shift()!;
    const parents = groups.filter(
      (group) => group.selected === selectedTag && !visited.has(group.name),
    );
    for (const parent of parents) {
      updates.push({
        tag: parent.name,
        delayMs,
        reachable,
        at,
        selectedTag,
      });
      visited.add(parent.name);
      queue.push(parent.name);
    }
  }

  return updates;
}

/** Add selected-parent projections to a set of authoritative probe updates. */
export function projectSelectedGroupHistoryUpdates(
  groups: PolicyGroup[],
  updates: PolicyProbeHistoryUpdate[],
): PolicyProbeHistoryUpdate[] {
  const expanded: PolicyProbeHistoryUpdate[] = [];
  const seen = new Set<string>();

  for (const update of updates) {
    for (const projected of buildProbeHistoryUpdates(
      groups,
      update.tag,
      update.delayMs,
      update.reachable,
      update.at,
    )) {
      const authoritative = projected.tag === update.tag ? update : projected;
      const key = [
        authoritative.tag,
        authoritative.at,
        authoritative.delayMs ?? '',
        authoritative.reachable,
        authoritative.selectedTag ?? '',
      ].join('\u0000');
      if (seen.has(key)) continue;
      seen.add(key);
      expanded.push(authoritative);
    }
  }

  return expanded;
}

/**
 * Project one authoritative policy.probe.completed snapshot into latency
 * history updates. Member results stay keyed by member tag; the policy-group
 * entry is the result of the member named by `selected` in this same event.
 */
export function buildPolicyProbeHistoryUpdates(
  probe: PolicyProbeCompletedEvent,
  fallbackAt = Date.now(),
): PolicyProbeHistoryUpdate[] {
  const completedAt = probe.completedAtUnixMs ?? fallbackAt;
  const updates = probe.members
    .filter((member) => member.tag.length > 0)
    .map<PolicyProbeHistoryUpdate>((member) => ({
      tag: member.tag,
      delayMs: member.delayMs,
      reachable: member.alive !== false,
      at: member.lastCheckedUnixMs ?? completedAt,
    }));

  if (!probe.policyTag || !probe.selected) return updates;

  const selected = probe.members.find((member) => member.tag === probe.selected);
  if (!selected) return updates;

  updates.push({
    tag: probe.policyTag,
    delayMs: selected.delayMs,
    reachable: selected.alive !== false,
    at: selected.lastCheckedUnixMs ?? completedAt,
    selectedTag: selected.tag,
  });
  return updates;
}
