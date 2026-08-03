import type { PolicyGroup } from '$lib/types/gui-api';

/** Keep only runtime state that can belong to the active config skeleton. */
export function retainConfiguredPolicyGroups(
  runtimeGroups: PolicyGroup[],
  configGroups: PolicyGroup[],
): PolicyGroup[] {
  const configuredTags = new Set(configGroups.map((group) => group.name));
  return runtimeGroups.filter((group) => configuredTags.has(group.name));
}

/**
 * Reject late probe events from a profile that is no longer active.
 * Runtime-only installations without a config skeleton may still update an
 * already-known group, but an unknown group is never created from an event.
 */
export function shouldApplyPolicyProbeEvent(
  configGroups: PolicyGroup[],
  runtimeGroups: PolicyGroup[],
  policyTag: string,
): boolean {
  const configured = configGroups.some((group) => group.name === policyTag);
  if (configGroups.length > 0) return configured;
  return runtimeGroups.some((group) => group.name === policyTag);
}
