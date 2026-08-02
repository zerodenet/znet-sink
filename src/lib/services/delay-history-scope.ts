import type { ConfigProxyNode, PolicyGroup } from '$lib/types/gui-api';

function hashText(value: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(36);
}

/**
 * Build a stable delay-history namespace for the active proxy configuration.
 *
 * The profile id separates two saved configurations that reuse the same node
 * and policy names. The structure fingerprint also changes immediately when
 * the active config is reloaded before the GUI self-test snapshot has caught
 * up with the new profile id.
 */
export function buildDelayHistoryScope(
  activeProxyConfigId: string | undefined,
  configNodes: ConfigProxyNode[],
  configGroups: PolicyGroup[],
): string {
  const nodes = configNodes
    .map((node) => [
      node.tag,
      node.protocol,
      node.server ?? '',
      node.port ?? '',
    ].join('\u0001'))
    .sort();
  const groups = configGroups
    .map((group) => [
      group.name,
      group.kind ?? '',
      group.outbounds.map((member) => member.tag).join('\u0002'),
    ].join('\u0001'))
    .sort();
  const fingerprint = hashText([...nodes, '--groups--', ...groups].join('\u0003'));
  const profile = activeProxyConfigId?.trim() || 'unscoped';
  return `${profile}:${fingerprint}`;
}
