/**
 * @typedef {{ id: string, tag: string }} ProbeNode
 */

/**
 * Pure state tracker for a batch node probe session.
 * It models the exact contract the UI needs:
 * - all batch nodes start in probing state
 * - a probe result clears every visible node instance for that target tag
 * - completion/error clears any remaining probing state
 * @param {ProbeNode[]} nodes
 */
export function createBatchProbeState(nodes) {
  /** @type {Map<string, string[]>} */
  const pendingNodeIdsByTag = new Map();
  /** @type {Set<string>} */
  const probingNodeIds = new Set();

  for (const node of nodes) {
    probingNodeIds.add(node.id);
    const ids = pendingNodeIdsByTag.get(node.tag) ?? [];
    ids.push(node.id);
    pendingNodeIdsByTag.set(node.tag, ids);
  }

  return {
    /**
     * Snapshot of every node id that should currently render as probing.
     * The returned Set is a copy so callers cannot mutate internal state.
     */
    probingNodeIds() {
      return new Set(probingNodeIds);
    },

    /**
     * Mark one target tag as having received a result.
     * Returns the affected node ids.
     */
    /**
     * @param {string} targetTag
     */
    resolveTag(targetTag) {
      const ids = pendingNodeIdsByTag.get(targetTag) ?? [];
      for (const id of ids) probingNodeIds.delete(id);
      pendingNodeIdsByTag.delete(targetTag);
      return [...ids];
    },

    /**
     * Clear the whole batch, used on completion or fatal error.
     * Returns the ids that were still probing before the clear.
     */
    clear() {
      const remaining = [...probingNodeIds];
      probingNodeIds.clear();
      pendingNodeIdsByTag.clear();
      return remaining;
    },
  };
}
