import { browser } from '$app/environment';
import type { ThemeMode } from './theme.svelte';

export type UIMode = 'lite' | 'pro';

class AppStateStore {
  isInitialized = $state(false);
  uiMode = $state<UIMode>('lite');
  activeTab = $state('overview');
  currentProxyMode = $state('rule');
  selectedNodeId = $state('node-1');
  selectedTheme = $state<ThemeMode>('system');
  visibleTabs = $state<string[]>(['overview', 'profiles', 'subscriptions', 'rules', 'connections', 'logs', 'capabilities', 'settings']);

  constructor() {
    if (browser) {
      const savedMode = localStorage.getItem('znet-ui-mode') as UIMode | null;
      const savedInit = localStorage.getItem('znet-is-init');
      const savedTheme = localStorage.getItem('znet-theme') as ThemeMode | null;
      const savedVisibleTabs = localStorage.getItem('znet-visible-tabs');
      
      if (savedMode) this.uiMode = savedMode;
      if (savedInit === 'true') this.isInitialized = true;
      if (savedTheme) this.selectedTheme = savedTheme;
      if (savedVisibleTabs) this.visibleTabs = JSON.parse(savedVisibleTabs);
    }
  }

  // 🌟 2. 核心原子控制行为：首次快速开始
  startApp(mode: UIMode) {
    this.uiMode = mode;
    this.isInitialized = true;
    if (browser) {
      localStorage.setItem('znet-ui-mode', mode);
      localStorage.setItem('znet-is-init', 'true');
    }
  }

  // 🌟 3. 核心能力：允许用户在设置页面中随时“横跳切换模式”，动态重载 UI
  switchUIMode(mode: UIMode) {
    this.uiMode = mode;
    // 如果从专业模式切回简约模式，且当前处于专业版才有的菜单（如 rulesets），强行归位到概览页，防止界面死锁
    if (mode === 'lite' && (this.activeTab === 'rulesets' || this.activeTab === 'plugins')) {
      this.activeTab = 'overview';
    }
    if (browser) {
      localStorage.setItem('znet-ui-mode', mode);
    }
    console.log(`[GUI Control] 用户动态切换了客户端工作流模式至: ${mode}`);
  }

  toggleTabVisibility(tabId: string) {
    if (tabId === 'settings') return;
    const index = this.visibleTabs.indexOf(tabId);
    if (index > -1) {
      this.visibleTabs.splice(index, 1);
      if (this.activeTab === tabId) {
        this.activeTab = 'settings';
      }
    } else {
      this.visibleTabs.push(tabId);
    }
    if (browser) {
      localStorage.setItem('znet-visible-tabs', JSON.stringify(this.visibleTabs));
    }
  }

  resetApp() {
    this.isInitialized = false;
    this.activeTab = 'overview';
    this.selectedTheme = 'system';
    this.visibleTabs = ['overview', 'profiles', 'subscriptions', 'rules', 'connections', 'logs', 'capabilities', 'settings'];
    if (browser) {
      localStorage.removeItem('znet-is-init');
      localStorage.removeItem('znet-ui-mode');
      localStorage.removeItem('znet-theme');
      localStorage.removeItem('znet-visible-tabs');
    }
  }
}

// 导出全局唯一的单例 store
export const store = new AppStateStore();
