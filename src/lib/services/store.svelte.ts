import { browser } from '$app/environment';
import type { ThemeMode } from './theme.svelte';
import { getAppConfig, updateAppConfig, getGuiInteractionSurfaceSnapshot } from './core';
import type { InteractionSurfaceItem } from '$lib/types/capability';

export type UIMode = 'lite' | 'pro';
export type SettingsSection = 'general' | 'core' | 'config' | 'about';

class AppStateStore {
  isInitialized = $state(false);
  appLoading = $state(true);
  loadError = $state<string | null>(null);
  uiMode = $state<UIMode>('lite');
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

  constructor() {
    if (browser) {
      this.hydrateFromLocalStorage();
      if (localStorage.getItem('znet-reset') === '1') {
        this.isInitialized = false;
        this.appLoading = false;
      }
    }
  }

  private hydrateFromLocalStorage() {
    const savedMode = localStorage.getItem('znet-ui-mode') as UIMode | null;
    const savedTheme = localStorage.getItem('znet-theme') as ThemeMode | null;

    if (savedMode) this.uiMode = savedMode;
    if (savedTheme) this.selectedTheme = savedTheme;
  }

  /** Load app config from Rust backend and merge into store state. */
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
        navigation: new Map(surface.navigation.map(item => [item.key, item])),
        actions: new Map(surface.actions.map(item => [item.key, item])),
        features: new Map(surface.features.map(item => [item.key, item])),
      };

      if (config.ui.defaultRoute && this.isNavVisible(config.ui.defaultRoute)) {
        this.activeTab = config.ui.defaultRoute;
      }

      if (typeof localStorage !== 'undefined' && localStorage.getItem('znet-reset') === '1') {
        localStorage.removeItem('znet-reset');
      } else {
        this.isInitialized = true;
      }
    } catch (e) {
      this.loadError = `后端加载失败: ${e instanceof Error ? e.message : String(e)}`;
    } finally {
      this.appLoading = false;
    }
  }

  /** Persist theme to Rust backend (and localStorage fallback). */
  async persistTheme(theme: ThemeMode) {
    this.selectedTheme = theme;
    if (browser) {
      localStorage.setItem('znet-theme', theme);
    }
    try {
      await updateAppConfig({ ui: { theme } });
    } catch {
      // Backend may not be available
    }
  }

  async startApp(mode: UIMode) {
    this.uiMode = mode;
    if (browser) {
      localStorage.setItem('znet-ui-mode', mode);
    }
    await this.loadFromBackend();
    if (!this.isInitialized) {
      this.isInitialized = true;
    }
    await this.persistUiMode(mode);
  }

  openSettings(section: SettingsSection = 'core') {
    this.isInitialized = true;
    this.activeTab = 'settings';
    this.settingsSection = section;
  }

  async switchUIMode(mode: UIMode) {
    const previousMode = this.uiMode;
    console.time('[ZNet] switchUIMode');

    // Optimistic update — UI responds instantly
    this.uiMode = mode;
    if (browser) {
      localStorage.setItem('znet-ui-mode', mode);
    }

    try {
      // Both operations are independent — run in parallel
      await Promise.all([
        this.persistUiMode(mode),
        this.refreshInteractionSurface(),
      ]);

      // If current tab is no longer visible after surface refresh, navigate away
      const navItem = this.interactionSurface.navigation.get(this.activeTab);
      if (!navItem?.visible) {
        this.activeTab = 'overview';
      }

      console.timeEnd('[ZNet] switchUIMode');
    } catch (e) {
      console.error('[ZNet] switchUIMode failed:', e);
      console.timeEnd('[ZNet] switchUIMode');
      // Revert optimistic update on failure
      this.uiMode = previousMode;
      if (browser) {
        localStorage.setItem('znet-ui-mode', previousMode);
      }
    }
  }

  async refreshInteractionSurface() {
    try {
      console.time('[ZNet] refreshInteractionSurface');
      const surface = await getGuiInteractionSurfaceSnapshot();
      this.interactionSurface = {
        navigation: new Map(surface.navigation.map(item => [item.key, item])),
        actions: new Map(surface.actions.map(item => [item.key, item])),
        features: new Map(surface.features.map(item => [item.key, item])),
      };
      console.timeEnd('[ZNet] refreshInteractionSurface');
    } catch (e) {
      console.warn('[ZNet] refreshInteractionSurface failed:', e);
    }
  }

  private getFallbackNavVisible(key: string): boolean {
    // 后端不可用时，简约模式默认可见的导航
    // nodes 由 Rust interaction_surface 根据活跃配置动态控制，不在此硬编码
    const liteModeNav = ['overview', 'profiles', 'subscriptions', 'settings'];
    return liteModeNav.includes(key);
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
    return item?.operable ?? this.getFallbackNavVisible(key);
  }

  isFeatureVisible(key: string): boolean {
    const item = this.interactionSurface.features.get(key);
    if (item) return item.visible;
    // 简约模式默认隐藏高级功能
    const liteModeFeatures = ['connections'];
    return liteModeFeatures.includes(key);
  }

  private async persistUiMode(mode: UIMode) {
    try {
      await updateAppConfig({ ui: { uiMode: mode } });
    } catch (e) {
      console.warn('[ZNet] persistUiMode failed:', e);
    }
  }

  resetApp() {
    this.isInitialized = false;
    this.activeTab = 'overview';
    this.settingsSection = 'general';
    this.selectedTheme = 'system';
    if (browser) {
      localStorage.removeItem('znet-is-init');
      localStorage.removeItem('znet-ui-mode');
      localStorage.removeItem('znet-theme');
      localStorage.setItem('znet-reset', '1');
    }
  }
}

export const store = new AppStateStore();
