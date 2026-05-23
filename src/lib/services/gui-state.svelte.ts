import { browser } from '$app/environment';
import { 
  getGuiSelfTestSnapshot,
  getGuiConnectionStatus,
  guiConnect,
  guiDisconnect,
  getGuiProxyModeStatus,
  guiSetProxyMode,
  getGuiCoreOverview,
  getGuiTrafficStats,
  getGuiPolicyGroups,
} from './core';
import type { 
  SelfTestSnapshot, 
  ConnectionStatus, 
  ProxyModeStatus, 
  CoreOverview, 
  TrafficStats, 
  PolicyGroup,
  ProxyMode 
} from '$lib/types/gui-api';

class GuiStateStore {
  selfTest = $state<SelfTestSnapshot | null>(null);
  connection = $state<ConnectionStatus | null>(null);
  proxyMode = $state<ProxyModeStatus | null>(null);
  coreOverview = $state<CoreOverview | null>(null);
  trafficStats = $state<TrafficStats | null>(null);
  policyGroups = $state<PolicyGroup[]>([]);
  
  isLoading = $state(false);
  isConnecting = $state(false);
  isDisconnecting = $state(false);
  isSwitchingMode = $state(false);
  
  private pollingInterval: number | null = null;
  private isInitialized = false;

  async initialize() {
    if (this.isInitialized) return;
    this.isInitialized = true;
    
    await this.refreshAll();
    this.startPolling();
  }

  async refreshAll() {
    try {
      await Promise.allSettled([
        this.refreshSelfTest(),
        this.refreshConnectionStatus(),
        this.refreshProxyMode(),
        this.refreshCoreOverview(),
        this.refreshTrafficStats(),
        this.refreshPolicyGroups(),
      ]);
    } catch {
      // 后端可能不可用，静默失败
    }
  }

  async refreshSelfTest() {
    try {
      this.selfTest = await getGuiSelfTestSnapshot();
    } catch {
      this.selfTest = null;
    }
  }

  async refreshConnectionStatus() {
    try {
      this.connection = await getGuiConnectionStatus();
    } catch {
      this.connection = null;
    }
  }

  async refreshProxyMode() {
    try {
      this.proxyMode = await getGuiProxyModeStatus();
    } catch {
      this.proxyMode = null;
    }
  }

  async refreshCoreOverview() {
    try {
      this.coreOverview = await getGuiCoreOverview();
    } catch {
      this.coreOverview = null;
    }
  }

  async refreshTrafficStats() {
    try {
      this.trafficStats = await getGuiTrafficStats();
    } catch {
      this.trafficStats = null;
    }
  }

  async refreshPolicyGroups() {
    try {
      this.policyGroups = await getGuiPolicyGroups();
    } catch {
      this.policyGroups = [];
    }
  }

  async connect() {
    this.isConnecting = true;
    try {
      this.connection = await guiConnect();
      await this.refreshProxyMode();
    } finally {
      this.isConnecting = false;
    }
  }

  async disconnect() {
    this.isDisconnecting = true;
    try {
      this.connection = await guiDisconnect();
    } finally {
      this.isDisconnecting = false;
    }
  }

  async setProxyMode(mode: ProxyMode) {
    this.isSwitchingMode = true;
    try {
      this.proxyMode = await guiSetProxyMode(mode, true);
      await this.refreshConnectionStatus();
    } finally {
      this.isSwitchingMode = false;
    }
  }

  private startPolling() {
    if (!browser) return;
    
    this.stopPolling();
    this.pollingInterval = window.setInterval(() => {
      Promise.allSettled([
        this.refreshConnectionStatus(),
        this.refreshTrafficStats(),
        this.refreshCoreOverview(),
      ]);
    }, 2000);
  }

  private stopPolling() {
    if (this.pollingInterval !== null) {
      clearInterval(this.pollingInterval);
      this.pollingInterval = null;
    }
  }

  destroy() {
    this.stopPolling();
    this.isInitialized = false;
  }

  get canConnect(): boolean {
    return !!this.selfTest?.ready && !this.isConnecting && !this.isDisconnecting && this.connection?.state !== 'connected';
  }

  get canDisconnect(): boolean {
    return !this.isConnecting && !this.isDisconnecting && this.connection?.state === 'connected';
  }

  get blockingIssues(): string[] {
    return this.selfTest?.blockingIssues ?? [];
  }
}

export const guiState = new GuiStateStore();
