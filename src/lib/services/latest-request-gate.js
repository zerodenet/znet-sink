/** @typedef {{ generation: number; requestId: number }} RequestToken */

/**
 * Small request-order gate for views that can refresh from timers, events,
 * filters, and manual actions at the same time.
 *
 * A generation invalidates requests issued for an older query. Within one
 * generation, a response may apply only when no newer response has already
 * been committed.
 */
export function createLatestRequestGate() {
  let generation = 0;
  let nextRequestId = 0;
  let latestAppliedRequestId = 0;

  return {
    reset() {
      generation += 1;
      nextRequestId = 0;
      latestAppliedRequestId = 0;
      return generation;
    },

    /** @param {number} [requestGeneration] */
    begin(requestGeneration = generation) {
      nextRequestId += 1;
      return { generation: requestGeneration, requestId: nextRequestId };
    },

    /** @param {RequestToken} request */
    canApply(request) {
      if (request.generation !== generation || request.requestId < latestAppliedRequestId) {
        return false;
      }
      latestAppliedRequestId = request.requestId;
      return true;
    },

    /** @param {RequestToken} request */
    isLatest(request) {
      return request.generation === generation && request.requestId === nextRequestId;
    },

    /** @param {number} requestGeneration */
    isCurrentGeneration(requestGeneration) {
      return requestGeneration === generation;
    },
  };
}
