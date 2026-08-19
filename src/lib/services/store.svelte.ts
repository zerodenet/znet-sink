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
    const previousMode = this.uiMode;
    const previousTab = this.activeTab;
    const previousSettingsSection = this.settingsSection;
    console.time('[ZNet] switchUIMode');

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
      // Entering Lite does not invent a new active session. It only migrates
      // an already-active GUI system-proxy capture to TUN; an existing TUN is
      // reused and an entirely-off session remains off.
      if (mode === 'lite') {
        await guiState.prepareLiteCapture();
      }

      await this.persistUiMode(mode);
      await this.refreshInteractionSurface();

      const navItem = this.interactionSurface.navigation.get(this.activeTab);
      if (!navItem?.visible) {
        this.activeTab = 'overview';
      }

      console.timeEnd('[ZNet] switchUIMode');
    } catch (e) {
      console.error('[ZNet] switchUIMode failed:', e);
      console.timeEnd('[ZNet] switchUIMode');
      const failure = e as {
        code?: string;
        message?: string;
        details?: { error?: { code?: string } };
      };
      const insufficientPrivilege = failure?.code === 'insufficient_os_privilege'
        || failure?.details?.error?.code === 'insufficient_os_privilege';
      if (mode === 'lite' && insufficientPrivilege) {
        toastError(`无法切换到简约模式：${failure.message ?? 'TUN 启动需要更高的系统权限。'}`);
      }
      this.uiMode = previousMode;
      this.activeTab = previousTab;
      this.settingsSection = previousSettingsSection;
      if (browser) {
        localStorage.setItem('znet-ui-mode', previousMode);
      }
      // If persistence succeeded but a later surface refresh failed, restore
      // the backend mode as well instead of leaving frontend/backend split.
      try {
        await this.persistUiMode(previousMode);
      } catch {
        // Preserve the original mode-switch failure.
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