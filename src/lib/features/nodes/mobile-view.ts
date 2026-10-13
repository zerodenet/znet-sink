import type { PolicyGroup, ClientScope, ProbeJobSnapshot } from '$lib/types/gui-api';
import type { ProxyNode } from '$lib/types/protocol';
import { sameProbeScope } from '$lib/features/nodes/scope';
export { sameProbeScope, controlScope } from '$lib/features/nodes/scope';
import { filterNodes } from '$lib/components/tabs/nodes-view-model';

export function mobileGroups(groups: PolicyGroup[], nodes: ProxyNode[], query: string) {
  const term = query.trim().toLowerCase();
  return groups.flatMap(group => {
    const nameMatches = group.name.toLowerCase().includes(term);
    const members = filterNodes({ allNodes: nodes, groups, selectedGroup: group.name, query: nameMatches ? '' : term });
    return !term || nameMatches || members.length ? [{ group, members }] : [];
  });
}
export function ungroupedNodes(groups: PolicyGroup[], nodes: ProxyNode[], query: string) {
  const grouped = new Set(groups.flatMap(group => group.outbounds.map(member => member.tag)));
  const groupNames = new Set(groups.map(group => group.name));
  return filterNodes({ allNodes: nodes, groups, selectedGroup: null, query: query.trim().toLowerCase() })
    .filter(node => !grouped.has(node.tag) && !groupNames.has(node.tag));
}
/** Failed targets remain fixed to the original job; no new group members join a retry. */
export function failedProbeTargets(job: ProbeJobSnapshot, scope: ClientScope): string[] {
  if (!sameProbeScope(job.scope, scope) || !['failed', 'partially_failed', 'timed_out'].includes(job.state)) return [];
  const succeeded = new Set(job.results.filter(result => result.reachable).map(result => result.targetTag));
  return [...new Set(job.targetTags.filter(tag => !succeeded.has(tag)))];
}

export function latestProbeJob(current: ProbeJobSnapshot | null, candidate: ProbeJobSnapshot): ProbeJobSnapshot {
  if (!current) return candidate;
  if (current.id===candidate.id) {
    if (current.updatedAtUnixMs>candidate.updatedAtUnixMs || (current.state!=='running' && candidate.state==='running')) return current;
    return candidate;
  }
  return current.startedAtUnixMs>candidate.startedAtUnixMs || (current.startedAtUnixMs===candidate.startedAtUnixMs && current.id>candidate.id) ? current : candidate;
}
