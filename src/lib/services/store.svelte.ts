import { browser } from '$app/environment';
import type { ThemeMode } from './theme.svelte';
import { getAppConfig, updateAppConfig, getGuiInteractionSurfaceSnapshot } from './core';
import { error as toastError } from './toast.svelte';
import {
  completeOnboarding,
  isOnboardingRequired,
  resetOnboarding as resetOnboardingStorage,
} from './onboarding';
import type { InteractionSurfaceItem } from '$lib/types/capability';

export type UIMode = 'lite' | 'pro';
export type SettingsSection = 'general' | 'network' | 'core' | 'tun' | 'dns' | 'config' | 'logs' | 'about';

const LITE_MODE_NAV = new Set(['overview', 'nodes', 'profiles', 'logs', 'plugins', 'settings']);

class AppStateStore {
  isInitialized = $state(false);
  appLoading = $state(true);
  loadError = $state<string | null>(null);
  uiMode = $state<UIMode>('lite');
  /** Mobile navigation preserves the full management surface independently of desktop Lite. */
  mobilePresentation = $state(false);
  isSwitchingUiMode = $state(false);
  activeTab = $state('overview');
  settingsSection = $state<SettingsSection>('general');
  private returnLocation: { tab: string; section: SettingsSection } | null = null;
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
  private interactionSurfaceGeneration = 0;
  private uiModePersistence: Promise<unknown> = Promise.resolve();

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
    const surfaceGeneration = ++this.interactionSurfaceGeneration;
    try {
      const [config, surface] = await Promise.all([
        getAppConfig(),
        getGuiInteractionSurfaceSnapshot(this.mobilePresentation ? 'mobile' : 'desktop'),
      ]);

      if (config.ui.theme && ['light', 'dark', 'system'].includes(config.ui.theme)) {
        this.selectedTheme = config.ui.theme as ThemeMode;
      }
      if (config.ui.uiMode && ['lite', 'pro'].includes(config.ui.uiMode)) {
        this.uiMode = config.ui.uiMode as UIMode;
      }

      if (surfaceGeneration === this.interactionSurfaceGeneration) {
        this.interactionSurface = {
          navigation: new Map(surface.navigation.map((item) => [item.key, item])),
          actions: new Map(surface.actions.map((item) => [item.key, item])),
          features: new Map(surface.features.map((item) => [item.key, item])),
        };
      }

      if (config.ui.defaultRoute && this.isNavVisible(config.ui.defaultRoute === 'subscriptions' ? 'profiles' : config.ui.defaultRoute)) {
        this.activeTab = config.ui.defaultRoute === 'subscriptions' ? 'profiles' : config.ui.defaultRoute;
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
    this.openTab('settings');
    this.settingsSection = section;
  }

  openTab(tab: string) {
    if (tab === 'subscriptions') tab = 'profiles';
    if (this.activeTab !== tab && this.isNavVisible(this.activeTab)) {
      this.returnLocation = { tab: this.activeTab, section: this.settingsSection };
    }
    this.isInitialized = true;
    this.activeTab = tab;
  }

  returnToPreviousPage() {
    const target = this.returnLocation;
    this.returnLocation = null;
    this.activeTab = target?.tab ?? 'overview';
    if (target) this.settingsSection = target.section;
  }

  async switchUIMode(mode: UIMode) {
    if (this.isSwitchingUiMode || mode === this.uiMode) return;

    this.isSwitchingUiMode = true;
    const generation = ++this.uiModeGeneration;
    console.time('[ZNet] switchUIMode');

    // UI mode changes presentation only; keep the current capture session.
    // Persist the preference without holding the WebView in the old layout.
    this.uiMode = mode;
    if (browser) {
      localStorage.setItem('znet-ui-mode', mode);
    }

    // Persist in the background. The presentation switch must not stay
    // disabled while an IPC call or an older backend is slow/unavailable.
    void this.persistUiMode(mode)
      .then(() => {
        if (generation !== this.uiModeGeneration || this.uiMode !== mode) return;

        // Interaction surfaces are advisory metadata; refresh them after the
        // preference has been persisted and ignore stale responses.
        void this.refreshInteractionSurface(mode);
      })
      .catch((e) => {
        console.error('[ZNet] switchUIMode failed:', e);
        // Keep the local choice usable even when persistence fails. The toast
        // makes the durability issue visible without undoing the UI switch.
        if (generation === this.uiModeGeneration && this.uiMode === mode) {
          toastError(`已切换到${mode === 'lite' ? '简约' : '专业'}模式，但保存设置失败：${e instanceof Error ? e.message : String(e)}`);
        }
      })
      .finally(() => {
        if (generation === this.uiModeGeneration) {
          this.isSwitchingUiMode = false;
        }
        console.timeEnd('[ZNet] switchUIMode');
      });

    // Make the control interactive again immediately after scheduling IPC.
    this.isSwitchingUiMode = false;
  }

  async refreshInteractionSurface(expectedMode?: UIMode) {
    const generation = ++this.interactionSurfaceGeneration;
    try {
      console.time('[ZNet] refreshInteractionSurface');
      const surface = await getGuiInteractionSurfaceSnapshot(this.mobilePresentation ? 'mobile' : 'desktop');
      if (generation !== this.interactionSurfaceGeneration || (expectedMode && this.uiMode !== expectedMode)) {
        console.timeEnd('[ZNet] refreshInteractionSurface');
        return;
      }
      this.interactionSurface = {
        navigation: new Map(surface.navigation.map((item) => [item.key, item])),
        actions: new Map(surface.actions.map((item) => [item.key, item])),
        features: new Map(surface.features.map((item) => [item.key, item])),
      };

      // Visibility controls navigation discovery, never an already open page.
      console.timeEnd('[ZNet] refreshInteractionSurface');
    } catch (e) {
      console.warn('[ZNet] refreshInteractionSurface failed:', e);
    }
  }

  private getFallbackNavVisible(key: string): boolean {
    return LITE_MODE_NAV.has(key);
  }

  isNavVisible(key: string): boolean {
    if (key === 'subscriptions') return false;
    // Apply the local mode boundary synchronously. The cached interaction
    // surface still describes the previous mode until its background refresh
    // completes, which can take seconds when the core is busy.
    if (!this.mobilePresentation && this.uiMode === 'lite' && !LITE_MODE_NAV.has(key)) return false;

    const item = this.interactionSurface.navigation.get(key);
    if (this.mobilePresentation) return true;
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

  private persistUiMode(mode: UIMode) {
    // Serialize writes while keeping layout changes immediate. A slow older
    // request must not become the preference loaded at the next launch.
    const pending = this.uiModePersistence.catch(() => {}).then(() => updateAppConfig({ ui: { uiMode: mode } }));
    this.uiModePersistence = pending;
    return pending;
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
