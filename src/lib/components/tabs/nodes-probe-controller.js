import { createBatchProbeState } from './nodes-probe-state.js';

/**
 * @typedef {{ id: string, tag: string }} ProbeNode
 * @typedef {{ done: number, total: number }} ProbeProgress
 */

/**
 * @typedef {{
 *   probingNodeIds: Set<string>,
 *   probingAll: boolean,
 *   probeProgress: ProbeProgress,
 *   lastError: string | null,
 * }} ProbeControllerState
 */

/**
 * @typedef {{
 *   listen: <T>(event: string, handler: (event: { payload: T }) => void | Promise<void>) => Promise<() => void>,
 *   probeNode: (targetTag: string) => Promise<{ targetTag: string, reachable: boolean, latencyMs?: number }>,
 *   probeAll: (targetTags: string[], sessionId: string) => Promise<void>,
 *   recordDelay: (targetTag: string, latencyMs: number | undefined, reachable: boolean) => void,
 *   refreshPolicyGroups: () => Promise<void>,
 *   onStateChange?: (state: ProbeControllerState) => void,
 * }} ProbeControllerDeps
 */

/** @param {ProbeControllerDeps} deps */
export function createNodesProbeController(deps) {
  /** @type {Set<string>} */
  let probingNodeIds = new Set();
  let probingAll = false;
  /** @type {ProbeProgress} */
  let probeProgress = { done: 0, total: 0 };
  /** @type {string | null} */
  let lastError = null;

  /** @type {(() => void) | null} */
  let activeProbeResultUnlisten = null;
  /** @type {(() => void) | null} */
  let activeProbeProgressUnlisten = null;
  /** @type {(() => void) | null} */
  let activeProbeCompleteUnlisten = null;
  /** @type {(() => void) | null} */
  let activeProbeCompletionResolve = null;
  /** @type {string | null} */
  let activeProbeSessionId = null;

  function createSessionId() {
    if (typeof globalThis.crypto?.randomUUID === 'function') {
      return globalThis.crypto.randomUUID();
    }
    return `probe-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  }

  /** @param {{ sessionId?: string }} payload */
  function isActiveSessionPayload(payload) {
    return Boolean(activeProbeSessionId && payload.sessionId === activeProbeSessionId);
  }

  function snapshot() {
    return {
      probingNodeIds: new Set(probingNodeIds),
      probingAll,
      probeProgress: { ...probeProgress },
      lastError,
    };
  }

  function emit() {
    deps.onStateChange?.(snapshot());
  }

  /** @param {Iterable<string>} ids */
  function addProbingNodeIds(ids) {
    const next = new Set(probingNodeIds);
    for (const id of ids) next.add(id);
    probingNodeIds = next;
  }

  /** @param {Iterable<string>} ids */
  function removeProbingNodeIds(ids) {
    const next = new Set(probingNodeIds);
    for (const id of ids) next.delete(id);
    probingNodeIds = next;
  }

  function cleanup() {
    activeProbeResultUnlisten?.();
    activeProbeProgressUnlisten?.();
    activeProbeCompleteUnlisten?.();
    activeProbeCompletionResolve?.();
    activeProbeResultUnlisten = null;
    activeProbeProgressUnlisten = null;
    activeProbeCompleteUnlisten = null;
    activeProbeCompletionResolve = null;
    activeProbeSessionId = null;
  }

  return {
    getState() {
      return snapshot();
    },

    cleanup,

    /**
     * @param {ProbeNode} node
     */
    async handleProbe(node) {
      if (probingAll || probingNodeIds.has(node.id)) return;
      addProbingNodeIds([node.id]);
      lastError = null;
      emit();

      try {
        const result = await deps.probeNode(node.tag);
        deps.recordDelay(node.tag, result.latencyMs, result.reachable);
        await deps.refreshPolicyGroups();
      } catch (error) {
        lastError = String(error);
      } finally {
        removeProbingNodeIds([node.id]);
        emit();
      }
    },

    /**
     * @param {ProbeNode[]} nodes
     */
    async handleProbeAll(nodes) {
      if (probingAll || probingNodeIds.size > 0) return;

      cleanup();

      const batchNodes = [...nodes];
      if (batchNodes.length === 0) {
        probingNodeIds = new Set();
        probingAll = false;
        probeProgress = { done: 0, total: 0 };
        lastError = null;
        emit();
        return;
      }
      const targetTags = batchNodes.map((node) => node.tag);
      const batchProbeState = createBatchProbeState(batchNodes);
      const sessionId = createSessionId();

      probingAll = true;
      probingNodeIds = batchProbeState.probingNodeIds();
      probeProgress = { done: 0, total: batchNodes.length };
      lastError = null;
      activeProbeSessionId = sessionId;
      emit();

      try {
        /** @type {(() => void) | null} */
        let resolveCompletion = null;
        /** @type {Promise<void>} */
        const completion = new Promise((resolve) => {
          resolveCompletion = () => resolve(undefined);
        });
        activeProbeCompletionResolve = resolveCompletion;

        activeProbeResultUnlisten = await deps.listen(
          'probe:result',
          /** @param {{ payload: { sessionId: string, targetTag: string, reachable: boolean, latencyMs?: number } }} event */
          (event) => {
            if (!isActiveSessionPayload(event.payload)) return;
            const { targetTag, reachable, latencyMs } = event.payload;
            deps.recordDelay(targetTag, latencyMs, reachable);
            batchProbeState.resolveTag(targetTag);
            probingNodeIds = batchProbeState.probingNodeIds();
            emit();
          },
        );

        activeProbeProgressUnlisten = await deps.listen(
          'probe:progress',
          /** @param {{ payload: { sessionId: string, done: number, total: number } }} event */
          (event) => {
            if (!isActiveSessionPayload(event.payload)) return;
            probeProgress = { done: event.payload.done, total: event.payload.total };
            emit();
          },
        );

        activeProbeCompleteUnlisten = await deps.listen(
          'probe:complete',
          /** @param {{ payload: { sessionId: string } }} event */
          async (event) => {
            if (!isActiveSessionPayload(event.payload)) return;
            batchProbeState.clear();
            probingNodeIds = batchProbeState.probingNodeIds();
            await deps.refreshPolicyGroups();
            probingAll = false;
            emit();
            resolveCompletion?.();
            activeProbeCompletionResolve = null;
          },
        );

        await deps.probeAll(targetTags, sessionId);
        await completion;
      } catch (error) {
        lastError = String(error);
        probingAll = false;
        batchProbeState.clear();
        probingNodeIds = batchProbeState.probingNodeIds();
        emit();
      } finally {
        cleanup();
      }
    },
  };
}
