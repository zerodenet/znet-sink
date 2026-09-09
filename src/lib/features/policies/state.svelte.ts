import { createLatestRequestGate } from '$lib/services/latest-request-gate.js';
import { retainConfiguredPolicyGroups, shouldApplyPolicyProbeEvent } from '$lib/services/node-state-reconcile';
import type { ConfigProxyNode, PolicyGroup, ProxyModeStatus } from '$lib/types/gui-api';
interface Ports {
 getGuiProxyModeStatus(): Promise<ProxyModeStatus>;
 getConfigProxyNodes(): Promise<ConfigProxyNode[]>;
 getConfigPolicyGroups(): Promise<PolicyGroup[]>;
 getGuiPolicyGroups(): Promise<PolicyGroup[]>;
 errorMessage(error: unknown): string;
}
export class PolicyState {
  proxyMode = $state<ProxyModeStatus | null>(null);
  policyGroups = $state<PolicyGroup[]>([]);
  policyGroupsUpdatedAt = $state(0);
  policyGroupsError = $state<string | null>(null);
  configNodes = $state<ConfigProxyNode[]>([]);
  configPolicyGroups = $state<PolicyGroup[]>([]);
  readonly configNodesRefreshGate = createLatestRequestGate();
  readonly configPolicyGroupsRefreshGate = createLatestRequestGate();
  readonly policyGroupsRefreshGate = createLatestRequestGate();
  readonly proxyModeRefreshGate = createLatestRequestGate();
  modeError = $state<string | null>(null);
  configNodesError = $state<string | null>(null);
  configGroupsError = $state<string | null>(null);
  get lastError() { return this.policyGroupsError || this.modeError || this.configNodesError || this.configGroupsError; }
  private lifecycle = 0;
  private ports: Ports;
  constructor(ports: Ports) { this.ports = ports; }
  invalidate() {
    this.lifecycle += 1;
    this.configNodesRefreshGate.reset(); this.configPolicyGroupsRefreshGate.reset();
    this.policyGroupsRefreshGate.reset(); this.proxyModeRefreshGate.reset();
  }
  async refreshProxyMode() {
    const request = this.proxyModeRefreshGate.begin();
    try {
      const mode = await this.ports.getGuiProxyModeStatus();
      if (this.proxyModeRefreshGate.canApply(request)) { this.proxyMode = mode; this.modeError = null; }
    } catch (error) {
      if (this.proxyModeRefreshGate.canApply(request)) { this.proxyMode = null; this.modeError = this.ports.errorMessage(error); }
    }
  }

  async refreshConfigNodes() {
    const request = this.configNodesRefreshGate.begin();
    try {
      const nodes = await this.ports.getConfigProxyNodes();
      if (this.configNodesRefreshGate.canApply(request)) {
        this.configNodes = nodes; this.configNodesError = null;
      }
    } catch (error) {
      if (this.configNodesRefreshGate.canApply(request)) this.configNodesError = this.ports.errorMessage(error);
      // Keep the last known-good config snapshot during a config reload.
    }
  }

  async refreshConfigPolicyGroups() {
    const request = this.configPolicyGroupsRefreshGate.begin();
    try {
      const groups = await this.ports.getConfigPolicyGroups();
      if (this.configPolicyGroupsRefreshGate.canApply(request)) {
        this.configPolicyGroups = groups; this.configGroupsError = null;
      }
    } catch (error) {
      if (this.configPolicyGroupsRefreshGate.canApply(request)) this.configGroupsError = this.ports.errorMessage(error);
      // Preserve the previous snapshot until a newer request succeeds.
    }
  }

  async refreshPolicyGroups() {
    const request = this.policyGroupsRefreshGate.begin();
    try {
      const groups = await this.ports.getGuiPolicyGroups();
      if (this.policyGroupsRefreshGate.canApply(request)) {
        this.policyGroups = groups;
        this.policyGroupsUpdatedAt = Date.now();
        this.policyGroupsError = null;
        return true;
      }
    } catch (e: any) {
      if (this.policyGroupsRefreshGate.canApply(request)) this.policyGroupsError = this.ports.errorMessage(e);
      console.warn('[gui-state] policy groups failed:', this.ports.errorMessage(e));
    }
    return false;
  }

  async refreshNodeStateAfterConfigChange() {
    const lifecycle = ++this.lifecycle;
    this.policyGroupsUpdatedAt = 0;
    this.configNodesRefreshGate.reset();
    this.configPolicyGroupsRefreshGate.reset();
    this.policyGroupsRefreshGate.reset();

    await Promise.allSettled([
      this.refreshConfigNodes(),
      this.refreshConfigPolicyGroups(),
    ]);

    if (lifecycle !== this.lifecycle) return;
    this.policyGroups = retainConfiguredPolicyGroups(this.policyGroups, this.configPolicyGroups);
    await this.refreshPolicyGroups();
  }

  applyPolicyProbeCompleted(event: import('$lib/types/gui-api').PolicyProbeCompletedEvent) {
    const existing = this.policyGroups.find((group) => group.name === event.policyTag);
    if (!shouldApplyPolicyProbeEvent(this.configPolicyGroups, this.policyGroups, event.policyTag)) return;
    this.policyGroupsRefreshGate.reset();
    const previousMembers = new Map(existing?.outbounds.map((member) => [member.tag, member]) ?? []);
    const outbounds = event.members.map((member) => ({
      ...previousMembers.get(member.tag),
      ...member,
      lastCheckedUnixMs: member.lastCheckedUnixMs ?? event.completedAtUnixMs,
    }));
    const updated = {
      ...existing,
      name: event.policyTag,
      kind: existing?.kind ?? 'url_test',
      selected: event.selected ?? existing?.selected,
      outbounds,
    };
    this.policyGroups = existing
      ? this.policyGroups.map((group) => group.name === event.policyTag ? updated : group)
      : [...this.policyGroups, updated];
  }

}
