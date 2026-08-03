import { createBatchProbeState } from './nodes-probe-state.js';

const MAX_CONCURRENT_PROBES = 8;
const MIN_BATCH_PROBE_TIMEOUT_MS = 30_000;
const BATCH_PROBE_TIMEOUT_PER_WAVE_MS = 15_000;
const MAX_BATCH_PROBE_TIMEOUT_MS = 5 * 60_000;

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
 *   probeNode: (targetTag: string) => Promise<{ targetTag: string, reachable: boolean, latencyMs?: number, message?: string }>,
 *   probeAll: (targetTags: string[], sessionId: string) => Promise<void>,
 *   recordDelay: (targetTag: string, latencyMs: number | undefined, reachable: boolean) => void,
 *   onProbeFailure?: (failure: { targetTag?: string, message: string, scope: 'single' | 'batch' }) => void,
 *   refreshPolicyGroups: () => Promise<void>,
 *   batchTimeoutMs?: number | ((total: number) => number),
 *   onStateChange?: (state: ProbeControllerState) => void,
 * }} ProbeControllerDeps
 */

/** @param {unknown} error */
function probeErrorMessage(error) {
  if (error instanceof Error) return error.message;
  if (error && typeof error === 'object' && 'message' in error && typeof error.message === 'string') {
    return error.message;
  }
  try {
    const serialized = JSON.stringify(error);
    if (serialized && serialized !== '{}') return serialized;
  } catch {
    // Fall through to the platform string representation.
  }
  return String(error);
}

/** @param {number} total */
function defaultBatchProbeTimeoutMs(total) {
  const waves = Math.max(1, Math.ceil(total / MAX_CONCURRENT_PROBES));
  const estimated = 15_000 + waves * BATCH_PROBE_TIMEOUT_PER_WAVE_MS;
  return Math.min(
    MAX_BATCH_PROBE_TIMEOUT_MS,
    Math.max(MIN_BATCH_PROBE_TIMEOUT_MS, estimated),
  );
}

/** @param {ProbeControllerDeps} deps @param {number} total */
function resolveBatchProbeTimeoutMs(deps, total) {
  const configured = typeof deps.batchTimeoutMs === 'function'
    ? deps.batchTimeoutMs(total)
    : deps.batchTimeoutMs;
  if (typeof configured === 'number' && Number.isFinite(configured) && configured > 0) {
    return configured;
  }
  return defaultBatchProbeTimeoutMs(total);
}

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
        if (!result.reachable) {
          deps.onProbeFailure?.({
            targetTag: result.targetTag || node.tag,
            message: result.message || '节点不可达',
            scope: 'single',
          });
        }
      } catch (error) {
        lastError = probeErrorMessage(error);
        deps.onProbeFailure?.({
          targetTag: node.tag,
          message: lastError,
          scope: 'single',
        });
      } finally {
        removeProbingNodeIds([node.id]);
        emit();
        void deps.refreshPolicyGroups();
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
      const timeoutMs = resolveBatchProbeTimeoutMs(deps, batchNodes.length);
      /** @type {ReturnType<typeof setTimeout> | null} */
      let timeoutHandle = null;

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

        const finishBatch = () => {
          batchProbeState.clear();
          probingNodeIds = batchProbeState.probingNodeIds();
          probingAll = false;
          probeProgress = { done: batchNodes.length, total: batchNodes.length };
          emit();
          resolveCompletion?.();
          activeProbeCompletionResolve = null;
        };

        activeProbeResultUnlisten = await deps.listen(
          'probe:result',
          /** @param {{ payload: { sessionId: string, targetTag: string, reachable: boolean, latencyMs?: number, message?: string } }} event */
          (event) => {
            if (!isActiveSessionPayload(event.payload)) return;
            const { targetTag, reachable, latencyMs, message } = event.payload;
            deps.recordDelay(targetTag, latencyMs, reachable);
            if (!reachable) {
              deps.onProbeFailure?.({
                targetTag,
                message: message || '节点不可达',
                scope: 'batch',
              });
            }
            batchProbeState.resolveTag(targetTag);
            probingNodeIds = batchProbeState.probingNodeIds();
            probeProgress = {
              done: Math.max(probeProgress.done, batchNodes.length - probingNodeIds.size),
              total: batchNodes.length,
            };
            // `probe:complete` is advisory. Every target result is already an
            // authoritative terminal state, so do not leave the UI spinning
            // forever merely because the final aggregate event was dropped.
            if (probingNodeIds.size === 0) {
              finishBatch();
            } else {
              emit();
            }
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
          (event) => {
            if (!isActiveSessionPayload(event.payload)) return;
            finishBatch();
          },
        );

        const watchdog = new Promise((_, reject) => {
          timeoutHandle = setTimeout(() => {
            reject(new Error(
              `batch probe timed out after ${timeoutMs}ms; ${probingNodeIds.size} target(s) still pending`,
            ));
          }, timeoutMs);
        });

        await Promise.race([
          (async () => {
            await deps.probeAll(targetTags, sessionId);
            await completion;
          })(),
          watchdog,
        ]);
      } catch (error) {
        lastError = probeErrorMessage(error);
        deps.onProbeFailure?.({
          message: lastError,
          scope: 'batch',
        });
        probingAll = false;
        batchProbeState.clear();
        probingNodeIds = batchProbeState.probingNodeIds();
        emit();
      } finally {
        if (timeoutHandle) clearTimeout(timeoutHandle);
        cleanup();
        void deps.refreshPolicyGroups();
      }
    },
  };
}
