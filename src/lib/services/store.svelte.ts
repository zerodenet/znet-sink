import { browser } from '$app/environment';
import type { ThemeMode } from './theme.svelte';
import { getAppConfig, updateAppConfig, getGuiInteractionSurfaceSnapshot } from './core';
import {
  completeOnboarding,
  isOnboardingRequired,
  resetOnboarding as resetOnboardingStorage,
} from './onboarding';
import type { InteractionSurfaceItem } from '$lib/types/capability';

export type UIMode = 'lite' | 'pro';
export type SettingsSection = 'general' | 'core' | 'config' | 'about';

const LITE_MODE_NAV = new Set(['overview', 'nodes', 'subscriptions', 'logs', 'settings']);

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

  /** Load app config from Rust backend and merge it into store state. */
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

  /** Persist theme to Rust backend and localStorage. */
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
    this.settingsSection = this.uiMode === 'lite' && section === 'config' ? 'general' : section;
  }

  async switchUIMode(mode: UIMode) {
    if (this.isSwitchingUiMode || mode === this.uiMode) return;

    this.isSwitchingUiMode = true;
    const previousMode = this.uiMode;
    const previousTab = this.activeTab;
    const previousSettingsSection = this.settingsSection;
    console.time('[ZNet] switchUIMode');

    // Optimistic update so the UI responds instantly.
    this.uiMode = mode;
    if (browser) {
      localStorage.setItem('znet-ui-mode', mode);
    }
    // Tear down Pro-only pages before the backend mode changes. Otherwise
    // their reactive refreshes can race the permission change and dispatch a
    // burst of now-restricted IPC commands while the mode switch is pending.
    if (mode === 'lite' && !LITE_MODE_NAV.has(this.activeTab)) {
      this.activeTab = 'overview';
    }
    // The live kernel config editor is Pro-only even though Settings itself is
    // shared. Move to a safe section before the backend mode flips so the
    // editor cannot issue a now-restricted core.config request during teardown.
    if (mode === 'lite' && this.settingsSection === 'config') {
      this.settingsSection = 'general';
    }

    try {
      // The backend computes the interaction surface from its persisted mode.
      // Persist first, otherwise a Pro -> Lite switch can race and retain a
      // stale Pro-only active tab long enough to dispatch restricted commands.
      await this.persistUiMode(mode);
      await this.refreshInteractionSurface();

      // If the active tab is no longer visible after the surface refresh,
      // move the user back to a safe tab.
      const navItem = this.interactionSurface.navigation.get(this.activeTab);
      if (!navItem?.visible) {
        this.activeTab = 'overview';
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
      this.isSwitchingUiMode = false;
    }
  }

  async refreshInteractionSurface() {
    try {
      console.time('[ZNet] refreshInteractionSurface');
      const surface = await getGuiInteractionSurfaceSnapshot();
      this.interactionSurface = {
        navigation: new Map(surface.navigation.map((item) => [item.key, item])),
        actions: new Map(surface.actions.map((item) => [item.key, item])),
        features: new Map(surface.features.map((item) => [item.key, item])),
      };
      console.timeEnd('[ZNet] refreshInteractionSurface');
    } catch (e) {
      console.warn('[ZNet] refreshInteractionSurface failed:', e);
    }
  }

  private getFallbackNavVisible(key: string): boolean {
    // When capability metadata is unavailable, keep the Lite mode tabs usable.
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
    // Hide advanced features by default when capability metadata is missing.
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
