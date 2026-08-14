type ActiveConfigListener = () => void;

class ProxyConfigSignal {
  /**
   * Reactive revision for the persisted proxy-config collection.
   *
   * `listProxyConfigs()` reads this synchronously so existing Svelte effects
   * that load profiles automatically re-run after a successful mutation,
   * regardless of whether the mutation originated from Lite or Pro.
   */
  revision = $state(0);

  private activeListeners = new Set<ActiveConfigListener>();

  markChanged(activeSourceMayHaveChanged = false): void {
    this.revision += 1;
    if (!activeSourceMayHaveChanged) return;

    for (const listener of this.activeListeners) {
      listener();
    }
  }

  onActiveChanged(listener: ActiveConfigListener): () => void {
    this.activeListeners.add(listener);
    return () => this.activeListeners.delete(listener);
  }
}

export const proxyConfigSignal = new ProxyConfigSignal();
