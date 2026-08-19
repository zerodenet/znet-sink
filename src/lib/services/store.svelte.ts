import { browser } from '$app/environment';
import type { ThemeMode } from './theme.svelte';
import { getAppConfig, updateAppConfig, getGuiInteractionSurfaceSnapshot } from './core';
import { guiState } from './gui-state.svelte';
import { error as toastError } from './toast.svelte';
import {
  completeOnboarding,
  isOnboardingRequired,
  resetOnboarding as resetOnboardingStorage,
} from './onboarding';
import type { InteractionSurfaceItem } from '$lib/types/capability';

export type UIMode = 'lite' | 'pro';
export type SettingsSection = 'general' | 'network' | 'core' | 'tun' | 'config' | 'logs' | 'about';

const LITE_MODE_NAV = new Set(['overview', 'nodes', 'subscriptions', 'logs', 'settings']);
const PRO_ONLY_SETTINGS = new Set<SettingsSection>(['tun', 'config']);

class AppStateStore {
  isInitialized = $state(false);
  appLoading = $state(true);
  loadError = $state<string | null>(null);
  uiMode = $state<UIMode>('lite');
  isSwitchingUiMode = $state(false);
  activeTab = $state('overview');
  settingsSection = $state<SettingsSection>('general');
  selectedNodeId = $state('node-1');
  selectedTheme = $state<ThemeMode>('system');
  visibleTabs = $state<string[]>([]);
  interactionSurface = $state<{
    navigation: Map<string, InteractionSurfaceItem>;
    actions: Map<string, InteractionSurfaceItem>;
    features: Map<string, InteractionSurfaceItem>;
  }>({
    navigation: new Map(),
    actions: new Map(),
    features: new Map(),
  });
  private onboardingRequired = true;
  private uiModeGeneration = 0;

  constructor() {
    if (browser) {
      this.hydrateFromLocalStorage();
      this.onboardingRequired = isOnboardingRequired(localStorage);
    }
  }

  private hydrateFromLocalStorage() {
    const savedMode = localStorage.getItem('znet-ui-mode') as UIMode | null;
    const savedTheme = localStorage.getItem('znet-theme') as ThemeMode | null;

    if (savedMode) this.uiMode = savedMode;
    if (savedTheme) this.selectedTheme = savedTheme;
  }

  async loadFromBackend() {
    try {
      const [config, surface] = await Promise.all([
        getAppConfig(),
        getGuiInteractionSurfaceSnapshot(),
      ]);

      if (config.ui.theme && ['light', 'dark', 'system'].includes(config.ui.theme)) {
        this.selectedTheme = config.ui.theme as ThemeMode;
      }
      if (config.ui.uiMode && ['lite', 'pro'].includes(config.ui.uiMode)) {
        this.uiMode = config.ui.uiMode as UIMode;
      }

      this.interactionSurface = {
        navigation: new Map(surface.navigation.map((item) => [item.key, item])),
        actions: new Map(surface.actions.map((item) => [item.key, item])),
        features: new Map(surface.features.map((item) => [item.key, item])),
      };

      if (config.ui.defaultRoute && this.isNavVisible(config.ui.defaultRoute)) {
        this.activeTab = config.ui.defaultRoute;
      }

      this.isInitialized = !this.onboardingRequired;
    } catch (e) {
      this.loadError = `后端加载失败: ${e instanceof Error ? e.message : String(e)}`;
    } finally {
      this.appLoading = false;
    }
  }

  async persistTheme(theme: ThemeMode) {
    this.selectedTheme = theme;
    if (browser) {
      localStorage.setItem('znet-theme', theme);
    }
    try {
      await updateAppConfig({ ui: { theme } });
    } catch {
      // Backend may not be available.
    }
  }

  async startApp(mode: UIMode) {
    this.uiMode = mode;
    if (browser) {
      localStorage.setItem('znet-ui-mode', mode);
    }
    await this.persistUiMode(mode);
    if (browser) {
      completeOnboarding(localStorage);
    }
    this.onboardingRequired = false;
    this.isInitialized = true;
  }

  openSettings(section: SettingsSection = 'core') {
    this.isInitialized = true;
    this.activeTab = 'settings';
    this.settingsSection = this.uiMode === 'lite' && PRO_ONLY_SETTINGS.has(section)
      ? 'general'
      : section;
  }

  async switchUIMode(mode: UIMode) {
    if (this.isSwitchingUiMode || mode === this.uiMode) return;

    this.isSwitchingUiMode = true;
    const generation = ++this.uiModeGeneration;
    const previousMode = this.uiMode;
    const previousTab = this.activeTab;
    const previousSettingsSection = this.settingsSection;
    console.time('[ZNet] switchUIMode');

    // UI mode is a presentation preference. Apply it optimistically so a
    // potentially slow macOS/Linux network handoff can never freeze the mode
    // control or hold the WebView in the old layout.
    this.uiMode = mode;
    if (browser) {
      localStorage.setItem('znet-ui-mode', mode);
    }

    if (mode === 'lite' && !LITE_MODE_NAV.has(this.activeTab)) {
      this.activeTab = 'overview';
    }
    if (mode === 'lite' && PRO_ONLY_SETTINGS.has(this.settingsSection)) {
      this.settingsSection = 'general';
    }

    try {
      // Only persistence is part of the mode-switch transaction. Network
      // capture reconciliation is a separate runtime concern and runs after
      // the UI mode has committed.
      await this.persistUiMode(mode);

      // Interaction surfaces are advisory UI metadata. Refresh them without
      // keeping the segmented control disabled, and ignore a stale response if
      // the user has already switched modes again.
      void this.refreshInteractionSurface(mode);

      // Entering Lite should preserve an existing capture session by filling
      // its missing side (system proxy or TUN), but it must not make the mode
      // switch wait for OS network changes. When no capture is active there is
      // nothing to reconcile, so avoid every TUN/status round-trip entirely.
      if (mode === 'lite' && guiState.isCaptureEnabled) {
        void this.prepareLiteCaptureInBackground(generation);
      }

      console.timeEnd('[ZNet] switchUIMode');
    } catch (e) {
      console.error('[ZNet] switchUIMode failed:', e);
      console.timeEnd('[ZNet] switchUIMode');
      this.uiMode = previousMode;
      this.activeTab = previousTab;
      this.settingsSection = previousSettingsSection;
      if (browser) {
        localStorage.setItem('znet-ui-mode', previousMode);
      }
    } finally {
      if (generation === this.uiModeGeneration) {
        this.isSwitchingUiMode = false;
      }
    }
  }

  private async prepareLiteCaptureInBackground(generation: number) {
    try {
      await guiState.prepareLiteCapture();
    } catch (e) {
      // Network handoff failure does not invalidate the user's UI preference.
      // Suppress stale feedback if the user has already left Lite mode while
      // an OS-level TUN operation was still completing.
      if (generation !== this.uiModeGeneration || this.uiMode !== 'lite') return;

      const failure = e as {
        code?: string;
        message?: string;
        details?: { error?: { code?: string } };
      };
      const insufficientPrivilege = failure?.code === 'insufficient_os_privilege'
        || failure?.details?.error?.code === 'insufficient_os_privilege';
      const message = failure?.message
        ?? (insufficientPrivilege
          ? 'TUN 启动需要更高的系统权限。'
          : '当前代理状态未能自动调整。');
      toastError(`已切换到简约模式，但自动接管未完成：${message}`);
    }
  }

  async refreshInteractionSurface(expectedMode?: UIMode) {
    try {
      console.time('[ZNet] refreshInteractionSurface');
      const surface = await getGuiInteractionSurfaceSnapshot();
      if (expectedMode && this.uiMode !== expectedMode) {
        console.timeEnd('[ZNet] refreshInteractionSurface');
        return;
      }
      this.interactionSurface = {
        navigation: new Map(surface.navigation.map((item) => [item.key, item])),
        actions: new Map(surface.actions.map((item) => [item.key, item])),
        features: new Map(surface.features.map((item) => [item.key, item])),
      };

      const navItem = this.interactionSurface.navigation.get(this.activeTab);
      if (!navItem?.visible) {
        this.activeTab = 'overview';
      }
      console.timeEnd('[ZNet] refreshInteractionSurface');
    } catch (e) {
      console.warn('[ZNet] refreshInteractionSurface failed:', e);
    }
  }

  private getFallbackNavVisible(key: string): boolean {
    return LITE_MODE_NAV.has(key);
  }

  isNavVisible(key: string): boolean {
    const item = this.interactionSurface.navigation.get(key);
    if (item) return item.visible;
    return this.getFallbackNavVisible(key);
  }

  isNavOperable(key: string): boolean {
    const item = this.interactionSurface.navigation.get(key);
    return item?.operable ?? true;
  }

  isActionOperable(key: string): boolean {
    const item = this.interactionSurface.actions.get(key);
    return item?.operable ?? true;
  }

  isFeatureVisible(key: string): boolean {
    const item = this.interactionSurface.features.get(key);
    if (item) return item.visible;
    const liteModeFeatures = ['connections'];
    return liteModeFeatures.includes(key);
  }

  private async persistUiMode(mode: UIMode) {
    await updateAppConfig({ ui: { uiMode: mode } });
  }

  resetOnboarding() {
    this.onboardingRequired = true;
    this.isInitialized = false;
    this.activeTab = 'overview';
    this.settingsSection = 'general';
    if (browser) {
      resetOnboardingStorage(localStorage);
    }
  }
}

export const store = new AppStateStore();