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

export interface DelayHistoryScopeParts {
  profile: string;
  fingerprint: string;
}

/** Split at the final colon so profile ids remain free to contain colons. */
export function splitDelayHistoryScope(scope: string): DelayHistoryScopeParts {
  const separator = scope.lastIndexOf(':');
  if (separator < 0) return { profile: 'unscoped', fingerprint: scope };
  return {
    profile: scope.slice(0, separator) || 'unscoped',
    fingerprint: scope.slice(separator + 1),
  };
}

export interface DelayHistoryScopeTransition {
  migrateFrom?: string;
  provisionalScope: string | null;
}

/**
 * Plan migration between transient and authoritative scopes.
 *
 * During startup/config switching the structure and active profile id refresh
 * independently. A result can therefore be written briefly under an empty or
 * stale-id scope. We migrate only transitions that are provably transient:
 * unscoped -> identified with the same fingerprint, empty -> populated for
 * the same profile, or a previously marked provisional scope whose fingerprint
 * is later confirmed under a different profile id. A direct A -> B transition
 * with the same fingerprint is deliberately not migrated, preserving profile
 * isolation for two genuinely distinct but structurally identical profiles.
 */
export function planDelayHistoryScopeTransition(
  previousScope: string | null,
  candidateScope: string,
  provisionalScope: string | null,
  emptyFingerprint: string,
): DelayHistoryScopeTransition {
  if (!previousScope || previousScope === candidateScope) {
    return { provisionalScope };
  }

  const previous = splitDelayHistoryScope(previousScope);
  const candidate = splitDelayHistoryScope(candidateScope);

  if (
    previous.profile === 'unscoped'
    && previous.fingerprint === candidate.fingerprint
  ) {
    return { migrateFrom: previousScope, provisionalScope: null };
  }

  if (
    provisionalScope === previousScope
    && previous.fingerprint === candidate.fingerprint
    && previous.profile !== candidate.profile
  ) {
    return { migrateFrom: previousScope, provisionalScope: null };
  }

  if (
    previous.fingerprint === emptyFingerprint
    && (previous.profile === 'unscoped' || previous.profile === candidate.profile)
  ) {
    return {
      migrateFrom: previousScope,
      provisionalScope: previous.profile === candidate.profile ? candidateScope : null,
    };
  }

  if (
    previous.profile === candidate.profile
    && previous.fingerprint !== candidate.fingerprint
  ) {
    return { provisionalScope: candidateScope };
  }

  return { provisionalScope: null };
}
