import type { PolicyGroup, PolicyProbeCompletedEvent } from '$lib/types/gui-api';

export interface PolicyProbeHistoryUpdate {
  tag: string;
  delayMs?: number;
  reachable: boolean;
  at: number;
  selectedTag?: string;
}

const MIN_POLICY_PROBE_WAIT_MS = 60_000;
const POLICY_PROBE_WAIT_BASE_MS = 15_000;
const POLICY_PROBE_WAIT_PER_MEMBER_MS = 10_000;
const MAX_POLICY_PROBE_WAIT_MS = 10 * 60_000;

/**
 * A manual urltest request can be queued behind an already-running scheduled
 * cycle. Older kernels probe members serially with a five-second per-member
 * bound, so the GUI must allow for both the in-flight cycle and the requested
 * cycle instead of applying one fixed 60-second timeout to every group.
 */
export function policyProbeWaitTimeoutMs(memberCount: number): number {
  const count = Math.max(1, Math.floor(Number.isFinite(memberCount) ? memberCount : 1));
  const estimated = POLICY_PROBE_WAIT_BASE_MS + count * POLICY_PROBE_WAIT_PER_MEMBER_MS;
  return Math.min(MAX_POLICY_PROBE_WAIT_MS, Math.max(MIN_POLICY_PROBE_WAIT_MS, estimated));
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

/**
 * Recover probe history from an authoritative policy snapshot. This is used
 * after event-stream reconnects and ordinary policy refreshes so scheduled
 * urltest observations are not lost merely because their completion event was
 * missed while the GUI was disconnected or lagging.
 */
export function buildPolicySnapshotHistoryUpdates(
  groups: PolicyGroup[],
): PolicyProbeHistoryUpdate[] {
  const updates: PolicyProbeHistoryUpdate[] = [];

  for (const group of groups) {
    for (const member of group.outbounds) {
      const at = member.lastCheckedUnixMs;
      if (typeof at !== 'number' || !Number.isFinite(at)) continue;
      if (member.delayMs === undefined && member.alive === undefined && !member.lastError) continue;
      updates.push({
        tag: member.tag,
        delayMs: member.delayMs,
        reachable: member.alive !== false && !member.lastError,
        at,
      });
    }
  }

  return projectSelectedGroupHistoryUpdates(groups, updates);
}

/**
 * Convert a refreshed policy snapshot into the completion shape expected by a
 * pending waiter. The snapshot is accepted only when it contains a probe time
 * at or after this request, preventing an old cached result from hiding a real
 * timeout.
 */
export function policyProbeEventFromSnapshot(
  group: PolicyGroup | undefined,
  requestedAtUnixMs: number,
  trigger?: string,
): PolicyProbeCompletedEvent | undefined {
  if (!group) return undefined;
  const completedAtUnixMs = group.outbounds.reduce<number | undefined>((latest, member) => {
    const checkedAt = member.lastCheckedUnixMs;
    if (typeof checkedAt !== 'number' || !Number.isFinite(checkedAt)) return latest;
    return latest === undefined ? checkedAt : Math.max(latest, checkedAt);
  }, undefined);

  if (completedAtUnixMs === undefined || completedAtUnixMs < requestedAtUnixMs) return undefined;

  return {
    policyTag: group.name,
    ...(trigger ? { trigger } : {}),
    completedAtUnixMs,
    selected: group.selected,
    members: group.outbounds,
  };
}
