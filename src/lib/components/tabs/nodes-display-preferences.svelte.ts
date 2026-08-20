import type { PolicyGroup } from '$lib/types/gui-api';
import type { ProxyNode } from '$lib/types/protocol';

const HIDE_TIMEOUT_KEY = 'znet-nodes-hide-timeout';
const SORT_DELAY_KEY = 'znet-nodes-sort-delay';

class NodesDisplayPreferences {
  hideTimeout = $state(false);
  sortByDelay = $state(false);
  private initialized = false;

  load() {
    if (this.initialized) return;
    this.initialized = true;
    try {
      this.hideTimeout = localStorage.getItem(HIDE_TIMEOUT_KEY) === '1';
      this.sortByDelay = localStorage.getItem(SORT_DELAY_KEY) === '1';
    } catch {
      this.hideTimeout = false;
      this.sortByDelay = false;
    }
  }

  ensureLoaded() {
    this.load();
  }

  setHideTimeout(value: boolean) {
    this.ensureLoaded();
    this.hideTimeout = value;
    try {
      localStorage.setItem(HIDE_TIMEOUT_KEY, value ? '1' : '0');
    } catch {
      // View preference persistence is best effort.
    }
  }

  setSortByDelay(value: boolean) {
    this.ensureLoaded();
    this.sortByDelay = value;
    try {
      localStorage.setItem(SORT_DELAY_KEY, value ? '1' : '0');
    } catch {
      // View preference persistence is best effort.
    }
  }
}

export const nodesDisplayPreferences = new NodesDisplayPreferences();

export function isUrlTestGroup(group?: Pick<PolicyGroup, 'kind'>): boolean {
  return group?.kind?.toLowerCase() === 'urltest';
}

export function isDelaySortEnabled(): boolean {
  nodesDisplayPreferences.ensureLoaded();
  return nodesDisplayPreferences.sortByDelay;
}

export function isHideableTimeoutNode(
  node: Pick<ProxyNode, 'delay' | 'alive' | 'lastProbeAt' | 'selected'>,
): boolean {
  if (node.selected) return false;
  if (node.delay < 0) return true;
  return node.alive === false && Boolean(node.lastProbeAt);
}

export function matchesNodeHealthFilter(node: ProxyNode): boolean {
  nodesDisplayPreferences.ensureLoaded();
  return !nodesDisplayPreferences.hideTimeout || !isHideableTimeoutNode(node);
}

export function compareNodeDelay(a: ProxyNode, b: ProxyNode): number {
  const delayRank = (node: ProxyNode) => {
    if (node.delay >= 0) return 0;
    if (!node.lastProbeAt) return 1;
    return 2;
  };

  const rankDiff = delayRank(a) - delayRank(b);
  if (rankDiff !== 0) return rankDiff;

  if (a.delay >= 0 && b.delay >= 0) {
    return a.delay - b.delay;
  }

  return 0;
}
