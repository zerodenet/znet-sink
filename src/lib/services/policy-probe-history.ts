import type { PolicyProbeCompletedEvent } from '$lib/types/gui-api';

export interface PolicyProbeHistoryUpdate {
  tag: string;
  delayMs?: number;
  reachable: boolean;
  at: number;
  selectedTag?: string;
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
