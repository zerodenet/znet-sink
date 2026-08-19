import type { ProxyNode } from '$lib/types/protocol';

const HIDE_TIMEOUT_KEY = 'znet-nodes-hide-timeout';

class NodesDisplayPreferences {
  hideTimeout = $state(false);
  private initialized = false;

  load() {
    if (this.initialized) return;
    this.initialized = true;
    try {
      this.hideTimeout = localStorage.getItem(HIDE_TIMEOUT_KEY) === '1';
    } catch {
      this.hideTimeout = false;
    }
  }

  setHideTimeout(value: boolean) {
    this.hideTimeout = value;
    try {
      localStorage.setItem(HIDE_TIMEOUT_KEY, value ? '1' : '0');
    } catch {
      // View preference persistence is best effort.
    }
  }
}

export const nodesDisplayPreferences = new NodesDisplayPreferences();

/**
 * Hide only nodes with an actual failed observation:
 * - a negative delay is the explicit timeout sentinel;
 * - alive=false is only treated as failed after a probe observation exists.
 *
 * Untested nodes stay visible. A selected route also stays visible even when it
 * fails so the UI never conceals the route that is currently effective.
 */
export function isHideableTimeoutNode(
  node: Pick<ProxyNode, 'delay' | 'alive' | 'lastProbeAt' | 'selected'>,
): boolean {
  if (node.selected) return false;
  if (node.delay < 0) return true;
  return node.alive === false && Boolean(node.lastProbeAt);
}

export function matchesNodeHealthFilter(node: ProxyNode): boolean {
  return !nodesDisplayPreferences.hideTimeout || !isHideableTimeoutNode(node);
}
